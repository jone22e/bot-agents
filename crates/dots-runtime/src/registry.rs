//! Execucoes em andamento: contexto por token (gateway), handle por execucao (cancelamento)
//! e esperas de aprovacao.

use std::collections::HashMap;
use std::sync::Mutex;

use dots_engines::ExecutionHandle;
use tokio::sync::oneshot;

use crate::policy::SharedContext;

#[derive(Default)]
pub struct ExecutionRegistry {
    by_token: Mutex<HashMap<String, SharedContext>>,
    handles: Mutex<HashMap<String, ExecutionHandle>>,
    approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl ExecutionRegistry {
    pub fn register(&self, ctx: SharedContext, handle: ExecutionHandle) {
        self.by_token.lock().unwrap().insert(ctx.token.clone(), ctx.clone());
        self.handles.lock().unwrap().insert(ctx.execution_id.clone(), handle);
    }

    pub fn unregister(&self, execution_id: &str, token: &str) {
        self.by_token.lock().unwrap().remove(token);
        self.handles.lock().unwrap().remove(execution_id);
    }

    pub fn context_by_token(&self, token: &str) -> Option<SharedContext> {
        self.by_token.lock().unwrap().get(token).cloned()
    }

    pub fn cancel(&self, execution_id: &str) -> bool {
        match self.handles.lock().unwrap().get(execution_id) {
            Some(h) => {
                h.cancel();
                true
            }
            None => false,
        }
    }

    pub fn wait_approval(&self, approval_id: &str) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.approvals.lock().unwrap().insert(approval_id.to_string(), tx);
        rx
    }

    pub fn resolve_approval(&self, approval_id: &str, approved: bool) -> bool {
        match self.approvals.lock().unwrap().remove(approval_id) {
            Some(tx) => tx.send(approved).is_ok(),
            None => false,
        }
    }

    pub fn drop_approval(&self, approval_id: &str) {
        self.approvals.lock().unwrap().remove(approval_id);
    }
}
