use super::{Incoming, platform};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

const PRIVATE: u64 = 1 << 63;
static NEXT: AtomicU64 = AtomicU64::new(PRIVATE);
pub(super) fn private(generation: u64) -> bool {
    generation & PRIVATE != 0
}

type Channel = (
    u64,
    mpsc::SyncSender<folio_ai::agent::AttachedCall>,
    mpsc::Sender<Incoming>,
);
static CHANNEL: Mutex<Option<Channel>> = Mutex::new(None);
pub(super) fn current(generation: u64) -> bool {
    CHANNEL
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|(id, _, _)| *id == generation)
}

pub(super) fn call(generation: u64, request: &Value) -> Value {
    let Some(name) = request["tool"].as_str() else {
        return json!({"ok":false,"error":"expected Folio tool"});
    };
    let (reply, rx) = mpsc::channel();
    {
        let channel = CHANNEL.lock().unwrap();
        let Some((_, calls, wake)) = channel.as_ref().filter(|(id, _, _)| *id == generation) else {
            return closed();
        };
        let call = folio_ai::agent::AttachedCall {
            name: name.into(),
            args: request["arguments"].clone(),
            reply,
        };
        if calls.try_send(call).is_err() {
            return json!({"ok":false,"error":"turn unavailable"});
        }
        let _ = wake.send(Incoming::AiPump);
    }
    rx.recv().unwrap_or_else(|_| closed())
}
fn closed() -> Value {
    json!({"ok":false,"error":"turn closed"})
}

pub(crate) struct TurnEndpoint {
    id: u64,
    pub file: PathBuf,
    endpoint: String,
}
impl TurnEndpoint {
    pub fn open(
        root: &Path,
        wake: &mpsc::Sender<Incoming>,
    ) -> Result<(Self, mpsc::Receiver<folio_ai::agent::AttachedCall>), String> {
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let token = folio_project::util::uuid_v4();
        let file = root.join(format!("ai-attach-{}.json", folio_project::util::uuid_v4()));
        let (calls, rx) = mpsc::sync_channel(8);
        let mut channel = CHANNEL.lock().unwrap();
        if channel.is_some() {
            return Err("another ACP turn is active".into());
        }
        let endpoint = platform::listen(root, id, token.clone(), wake.clone())
            .map_err(|_| "cannot open ACP endpoint")?;
        *channel = Some((id, calls, wake.clone()));
        drop(channel);
        let lease = Self { id, file, endpoint };
        let data =
            json!({"version":crate::attached::PROTOCOL,"endpoint":lease.endpoint,"token":token});
        platform::write_private(&lease.file, data.to_string().as_bytes())
            .map_err(|_| "cannot write ACP connection file")?;
        Ok((lease, rx))
    }
}
impl Drop for TurnEndpoint {
    fn drop(&mut self) {
        let mut channel = CHANNEL.lock().unwrap();
        if channel.as_ref().is_some_and(|(id, _, _)| *id == self.id) {
            channel.take();
        }
        drop(channel);
        let _ = std::fs::remove_file(&self.file);
        platform::wake(&self.endpoint);
    }
}
