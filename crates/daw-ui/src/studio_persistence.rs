// Durable project saves and recoverable background snapshots. No project
// or audio data is overwritten until a complete temporary file is synced.
use std::io::Write;
use std::hash::{Hash,Hasher};
use std::sync::mpsc as autosave_mpsc;
use std::time::Instant;

fn atomic_save_project(path:&Path, bytes:&[u8])->Result<(),String> {
    let Some(dir)=path.parent() else {return Err("Project path has no parent directory".into())};
    let timestamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e|e.to_string())?.as_nanos();
    let filename=path.file_name().unwrap_or_default().to_string_lossy();
    let tmp=dir.join(format!(".{filename}.{}.{}.tmp",std::process::id(),timestamp));
    let result=(||->Result<(),String>{
        let mut file=std::fs::OpenOptions::new()
            .write(true).create_new(true).open(&tmp)
            .map_err(|e|format!("Create save tempfile {}: {e}",tmp.display()))?;
        file.write_all(bytes).map_err(|e|format!("Write save tempfile: {e}"))?;
        file.sync_all().map_err(|e|format!("Sync save tempfile: {e}"))?;
        drop(file);
        std::fs::rename(&tmp,path).map_err(|e|format!("Commit project save: {e}"))?;
        Ok(())
    })();
    if result.is_err(){let _=std::fs::remove_file(&tmp);}
    result
}

impl Studio {
    fn autosave_path(&self)->PathBuf {
        let base=std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
            .join(".local/share/rustie/autosaves");
        let name=self.project.name.chars()
            .filter(|c|c.is_ascii_alphanumeric() || *c=='-' || *c=='_')
            .take(28).collect::<String>();
        let name=if name.is_empty(){"Untitled"}else{&name};
        let mut hasher=std::collections::hash_map::DefaultHasher::new();
        self.project_path.as_ref().map(|p|p.to_string_lossy().to_string())
            .unwrap_or_else(||self.project.name.clone()).hash(&mut hasher);
        base.join(format!("{name}-{:016x}.slate.json",hasher.finish()))
    }

    fn poll_autosave(&mut self,ctx:&egui::Context){
        if let Some(result)=self.autosave_job.as_ref().and_then(|rx|rx.try_recv().ok()){
            self.autosave_job=None;
            match result {
                Ok(_) => {}, // Do not replace useful UI status text with periodic autosave noise.
                Err(err)=>{self.error=Some(format!("Autosave warning: {err}"));}
            }
        }
        if !self.dirty || self.autosave_job.is_some() || self.recorder.is_some(){return;}
        if self.project==self.last_autosaved_project {return;}
        if self.last_autosave.elapsed()<Duration::from_secs(60){
            ctx.request_repaint_after(Duration::from_secs(5));
            return;
        }
        let data=match serde_json::to_vec_pretty(&self.project){
            Ok(data)=>data,
            Err(err)=>{self.error=Some(format!("Autosave serialization: {err}"));return;}
        };
        let path=self.autosave_path();
        let project=self.project.clone();
        let (tx,rx)=autosave_mpsc::channel();
        self.autosave_job=Some(rx);
        self.last_autosave=Instant::now();
        std::thread::spawn(move||{
            let result=path.parent().ok_or_else(||"Autosave directory missing".to_string())
                .and_then(|dir|std::fs::create_dir_all(dir).map_err(|e|e.to_string()))
                .and_then(|()|atomic_save_project(&path,&data));
            let _=tx.send(result.map(|()|path));
        });
        self.last_autosaved_project=project;
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn recover_autosave(&mut self){
        let path=self.autosave_path();
        if !path.is_file(){
            self.error=Some("No autosave found for the current project".into());return;
        }
        self.load_project_path(path.clone());
        if self.error.is_none(){
            self.project_path=None; // Save As, don't overwrite recovery file accidentally.
            self.dirty=true;
            self.status=format!("Recovered {}; use Save As to keep it",path.display());
        }
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn atomic_save_preserves_complete_file_and_leaves_no_temps(){
        let base=std::env::temp_dir().join(format!("rustie-atomic-save-{}",std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let dest=base.join("recording-project.slate.json");
        std::fs::write(&dest,b"{old}").unwrap();
        let next=b"{\"project\":\"new\",\"tracks\":[1,2,3]}";
        atomic_save_project(&dest,next).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(),next);
        assert_eq!(std::fs::read_dir(&base).unwrap().count(),1);
        let _=std::fs::remove_file(dest);
        let _=std::fs::remove_dir(base);
    }
}
