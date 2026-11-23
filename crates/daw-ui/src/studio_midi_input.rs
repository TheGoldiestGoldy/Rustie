// ALSA-backed live MIDI input. The callback is intentionally minimal: it
// decodes short MIDI packets and enqueues them without touching the audio graph.
// The egui/control thread owns all engine commands and the port connection.
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::sync::mpsc::{Receiver, TryRecvError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MidiInputEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    AllNotesOff { channel: u8 },
    Sustain { channel: u8, value: u8 },
    PitchBend { channel: u8, value: u16 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
}

fn decode_midi_message(bytes: &[u8]) -> Option<MidiInputEvent> {
    if bytes.len() < 2 {
        return None;
    }
    let status = bytes[0];
    let channel = status & 0x0f;
    let note = bytes[1];
    match status & 0xf0 {
        0x80 if bytes.len() >= 3 && note <= 127 =>
            Some(MidiInputEvent::NoteOff { channel, note }),
        0x90 if bytes.len() >= 3 && note <= 127 && bytes[2] <= 127 => {
            if bytes[2] == 0 {
                Some(MidiInputEvent::NoteOff { channel, note })
            } else {
                Some(MidiInputEvent::NoteOn { channel, note, velocity: bytes[2] })
            }
        }
        0xb0 if bytes.len() >= 3 && (note == 120 || note == 123) =>
            Some(MidiInputEvent::AllNotesOff { channel }),
        0xb0 if bytes.len() >= 3 && note == 64 && bytes[2] < 128 =>
            Some(MidiInputEvent::Sustain { channel, value: bytes[2] }),
        0xb0 if bytes.len() >= 3 && note < 128 && bytes[2] < 128 =>
            Some(MidiInputEvent::ControlChange { channel, controller: note, value: bytes[2] }),
        0xc0 if note < 128 => Some(MidiInputEvent::ProgramChange { channel, program: note }),
        0xe0 if bytes.len() >= 3 && note < 128 && bytes[2] < 128 =>
            Some(MidiInputEvent::PitchBend {
                channel,
                value: u16::from(note) | u16::from(bytes[2]) << 7,
            }),
        _ => None,
    }
}

impl Studio {
    fn midi_ports() -> Vec<String> {
        MidiInput::new("Rusty MIDI port listing").ok().map_or_else(Vec::new, |input| {
            input.ports().iter().filter_map(|port| input.port_name(port).ok()).collect()
        })
    }

    fn midi_reset_routing(&mut self) {
        self.midi_held.clear();
        self.keyboard_held.clear();
        self.keyboard_mouse_note = None;
        self.midi_sustain_node.fill(None);
        self.midi_pending_panics.clear();
    }

    fn midi_send(&mut self, node: u32, channel: u8, message: LiveMidiMessage) -> bool {
        let Some(driver) = self.driver.as_mut() else { return false };
        if self.midi_pending_panics.contains(&node) {
            return false;
        }
        if driver.controller().immediate(EngineCommand::LiveMidi { node, channel, message }).is_err() {
            self.midi_pending_panics.insert(node);
            return false;
        }
        true
    }

    fn midi_retry_panics(&mut self) {
        let Some(driver) = self.driver.as_mut() else {
            self.midi_reset_routing();
            return;
        };
        self.midi_pending_panics.retain(|&node| {
            driver.controller().immediate(EngineCommand::AllNotesOff { node }).is_err()
        });
    }

    fn midi_release_held_notes(&mut self) {
        self.midi_pending_panics.extend(self.midi_held.drain().map(|(_, node)| node));
        self.midi_pending_panics.extend(self.midi_sustain_node.iter_mut().filter_map(Option::take));
        self.midi_retry_panics();
    }

    fn midi_disconnect(&mut self) {
        self.finish_midi_take();
        self.midi_release_held_notes();
        self.midi_connection = None;
        self.midi_receiver = None;
        self.midi_overflow = Arc::default();
        self.midi_name = None;
        if !self.playing {
            self.driver = None;
        }
        self.status = "MIDI input disconnected".into();
    }

    fn midi_connect(&mut self, port_name: &str) {
        self.midi_disconnect();
        let result = (|| -> Result<_, String> {
            let mut input = MidiInput::new("Rusty input")
                .map_err(|e| format!("Initialize MIDI: {e}"))?;
            input.ignore(Ignore::None);
            let port = input.ports().into_iter()
                .find(|port| input.port_name(port).ok().as_deref() == Some(port_name))
                .ok_or_else(|| format!("MIDI port no longer available: {port_name}"))?;
            let (sender, receiver) = mpsc::sync_channel::<MidiInputEvent>(1024);
            let overflow = Arc::clone(&self.midi_overflow);
            let connection = input.connect(&port, "Rusty live keyboard", move |_at, bytes, _| {
                if let Some(message) = decode_midi_message(bytes)
                    && sender.try_send(message).is_err() {
                        overflow.store(true, Ordering::Release);
                    }
            }, ()).map_err(|e| format!("Connect MIDI: {e}"))?;
            Ok((connection, receiver))
        })();
        match result {
            Ok((connection, receiver)) => {
                self.midi_connection = Some(connection);
                self.midi_receiver = Some(receiver);
                self.midi_name = Some(port_name.to_owned());
                self.status = format!("MIDI input connected: {port_name}");
                self.error = None;
                if !self.playing {
                    self.start_midi_monitor();
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// When stopped, keep the audio callback running for keyboard notes, but
    /// do NOT play audio clips or advance the transport timeline.
    fn start_midi_monitor(&mut self) {
        if self.midi_connection.is_none() || self.playing {
            return;
        }
        self.driver = None;
        self.start_monitor_driver();
    }

    /// Follow the currently selected MIDI track, not whichever track happened
    /// to be selected when the keyboard was connected. NoteOff remembers its
    /// original node so changing selection cannot leave voices hanging.
    fn midi_selected_node(&self) -> Option<u32> {
        let id = self.midi_take.as_ref().map(|take| take.track_id)
            .or(self.selected_track)?;
        self.project.tracks.iter().any(|track| track.id == id
            && track.kind == TrackKind::Midi).then(|| Self::track_midi_node(id).ok()).flatten()
    }

    fn poll_midi_events(&mut self) {
        self.midi_retry_panics();
        let mut events = Vec::new();
        if let Some(receiver) = self.midi_receiver.as_ref() {
            for _ in 0..256 {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.error = Some("MIDI port disconnected: reconnect in Devices".into());
                        break;
                    }
                }
            }
        }
        if self.midi_overflow.swap(false, Ordering::AcqRel) {
            if let Some(receiver) = self.midi_receiver.as_ref() {
                // Drop the damaged input batch. The callback keeps filling
                // at most one bounded buffer while this bounded drain runs.
                for _ in 0..1024 {
                    if receiver.try_recv().is_err() {
                        break;
                    }
                }
            }
            self.finish_midi_take();
            self.midi_release_held_notes();
            self.error = Some("MIDI input overflow. Active notes were released.".into());
            return;
        }
        for event in events {
            let frame = self.midi_transport_frame();
            match event {
                MidiInputEvent::NoteOn { channel, note, velocity } => {
                    let Some(node) = self.midi_selected_node() else { continue; };
                    if let Some(old_node) = self.midi_held.remove(&(channel, note)) {
                        self.midi_send(old_node, channel, LiveMidiMessage::NoteOff(note));
                    }
                    if self.midi_send(node, channel, LiveMidiMessage::NoteOn(note, velocity)) {
                        self.midi_held.insert((channel, note), node);
                        if let Some(take) = self.midi_take.as_mut() {
                            take.note_on(channel, note, velocity, frame);
                        }
                    }
                }
                MidiInputEvent::NoteOff { channel, note } => {
                    if let Some(node) = self.midi_held.remove(&(channel, note)) {
                        self.midi_send(node, channel, LiveMidiMessage::NoteOff(note));
                        if let Some(take) = self.midi_take.as_mut() {
                            take.note_off(channel, note, frame);
                        }
                    }
                }
                MidiInputEvent::AllNotesOff { channel } => {
                    let keys = self.midi_held.keys().copied()
                        .filter(|(ch, _)| *ch == channel).collect::<Vec<_>>();
                    let mut nodes = std::collections::BTreeSet::new();
                    nodes.extend(self.midi_sustain_node[usize::from(channel)].take());
                    for key in keys {
                        if let Some(node) = self.midi_held.remove(&key) {
                            self.midi_send(node, channel, LiveMidiMessage::NoteOff(key.1));
                            nodes.insert(node);
                            if let Some(take) = self.midi_take.as_mut() {
                                take.note_off(channel, key.1, frame);
                            }
                        }
                    }
                    for node in nodes {
                        self.midi_send(node, channel, LiveMidiMessage::ChannelPanic);
                    }
                }
                MidiInputEvent::Sustain { channel, value } => {
                    let index = usize::from(channel);
                    let node = self.midi_sustain_node[index].or_else(|| self.midi_selected_node());
                    if let Some(node) = node {
                        let sent = self.midi_send(node, channel, LiveMidiMessage::Sustain(value));
                        self.midi_sustain_node[index] = (sent && value >= 64).then_some(node);
                    }
                }
                MidiInputEvent::PitchBend { channel, value } => {
                    if let Some(node) = self.midi_selected_node() {
                        self.midi_send(node, channel, LiveMidiMessage::PitchBend(value));
                    }
                }
                MidiInputEvent::ControlChange { channel, controller, value } => {
                    if let (Some(node), Some(driver)) =
                        (self.midi_selected_node(), self.driver.as_mut())
                    {
                        let _ = driver.controller().immediate(EngineCommand::ControlChange {
                            node, channel, controller, value,
                        });
                    }
                }
                MidiInputEvent::ProgramChange { channel, program } => {
                    if let (Some(node), Some(driver)) =
                        (self.midi_selected_node(), self.driver.as_mut())
                    {
                        let _ = driver.controller().immediate(EngineCommand::ProgramChange {
                            node, channel, program,
                        });
                    }
                }
            }
        }
    }

    fn midi_device_editor(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Hardware MIDI keyboard");
        ui.weak("Select a MIDI track: play notes while stopped, or press REC to capture an editable MIDI clip.");
        let ports = Self::midi_ports();
        let mut connect_to = None;
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("rustie_midi_input_port")
                .selected_text(self.midi_name.as_deref().unwrap_or("Disconnected"))
                .width(240.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.midi_name.is_none(), "Disconnected").clicked() {
                        connect_to = Some(None);
                    }
                    for name in &ports {
                        if ui.selectable_label(self.midi_name.as_ref() == Some(name), name)
                            .clicked() {
                            connect_to = Some(Some(name.clone()));
                        }
                    }
                }).response.on_hover_text("Choose a physical ALSA MIDI input. No port is opened until selected.");
            if ui.button("All notes off").on_hover_text("Release all held MIDI keyboard notes").clicked() {
                self.midi_release_held_notes();
            }
        });
        if let Some(target) = connect_to {
            match target {
                Some(name) => self.midi_connect(&name),
                None => self.midi_disconnect(),
            }
        }
        if ports.is_empty() { ui.weak("No ALSA MIDI input ports found"); }
        if self.midi_connection.is_some() {
            ui.small(format!("Connected • {} notes held • input latency depends on UI refresh and audio backend",
                self.midi_held.len()));
        }
    }
}

#[cfg(test)]
mod midi_input_tests {
    use super::*;
    #[test]
    fn decodes_cc64_and_14_bit_pitch_bend_on_the_input_channel() {
        assert_eq!(decode_midi_message(&[0xb5, 64, 127]), Some(
            MidiInputEvent::Sustain { channel: 5, value: 127 }));
        assert_eq!(decode_midi_message(&[0xb5, 64, 0]), Some(
            MidiInputEvent::Sustain { channel: 5, value: 0 }));
        assert_eq!(decode_midi_message(&[0xe5, 1, 65]), Some(
            MidiInputEvent::PitchBend { channel: 5, value: 8321 }));
        assert_eq!(decode_midi_message(&[0xe5, 128, 65]), None);
    }

    #[test]
    fn parses_note_on_off_velocity_zero_and_panic() {
        assert_eq!(decode_midi_message(&[0x92, 60, 110]), Some(
            MidiInputEvent::NoteOn { channel: 2, note: 60, velocity: 110 }));
        assert_eq!(decode_midi_message(&[0x92, 60, 0]), Some(
            MidiInputEvent::NoteOff { channel: 2, note: 60 }));
        assert_eq!(decode_midi_message(&[0x83, 42, 64]), Some(
            MidiInputEvent::NoteOff { channel: 3, note: 42 }));
        assert_eq!(decode_midi_message(&[0xb3, 123, 0]), Some(
            MidiInputEvent::AllNotesOff { channel: 3 }));
        assert_eq!(decode_midi_message(&[0xf8]), None);
        assert_eq!(decode_midi_message(&[0x90, 61]), None);
        assert_eq!(decode_midi_message(&[0x90, 128, 64]), None);
    }

    #[test]
    fn decodes_builtin_instrument_controls() {
        assert_eq!(decode_midi_message(&[0xb2, 1, 42]), Some(
            MidiInputEvent::ControlChange { channel: 2, controller: 1, value: 42 }));
        assert_eq!(decode_midi_message(&[0xc3, 5]), Some(
            MidiInputEvent::ProgramChange { channel: 3, program: 5 }));
        assert_eq!(decode_midi_message(&[0xc3, 128]), None);
    }
}

[executed on device: Johnnysstation (1200896c-45a0-4cb2-8dfa-479e25d803a4)]