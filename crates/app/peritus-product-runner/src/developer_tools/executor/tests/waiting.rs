use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize},
};

struct WaitingView {
    started: tokio::sync::Notify,
    available: tokio::sync::Notify,
    cancelled: AtomicBool,
    writable: AtomicBool,
    calls: AtomicUsize,
}

impl crate::ConversationView for WaitingView {
    fn revision(&self) -> u64 {
        1
    }
    fn render(&self) -> String {
        "Write the requested file".to_owned()
    }
    fn effective_permissions(&self) -> HostPermissions {
        if self.writable.load(Ordering::Acquire) {
            HostPermissions::all()
        } else {
            HostPermissions::none()
        }
    }
    fn checkpoint_before_workspace_mutation_async<'a>(
        &'a self,
        _: &'a Path,
        _: crate::WorkspaceMutationKind,
    ) -> crate::WorkspaceCheckpointFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.started.notify_one();
            self.available.notified().await;
            if self.cancelled.load(Ordering::Acquire) {
                Err("cancelled".to_owned())
            } else {
                Ok(())
            }
        })
    }
}

#[test]
fn checkpoint_wait_keeps_the_executor_alive_and_revalidates_permission() {
    for disposition in ["resume", "cancel", "revoke", "drift"] {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("before.txt"), "before\n").expect("file");
        let view = Arc::new(WaitingView {
            started: tokio::sync::Notify::new(),
            available: tokio::sync::Notify::new(),
            cancelled: AtomicBool::new(false),
            writable: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
        });
        let mut tools = writable_tools(workspace.path())
            .with_checkpoint_observer(Arc::new(|_| Ok(())))
            .with_checkpoint_view(view.clone());
        tools.protection_view = Some(view.clone());
        execute(&mut tools, "workspace_list", r#"{"path":""}"#);
        execute(&mut tools, "workspace_read", r#"{"path":"before.txt"}"#);
        let call = completed_call(
            "retained-waiting-call",
            "workspace_write",
            r#"{"path":"before.txt","content":"after\n"}"#,
        );
        let runtime =
            tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
        let observation = runtime.block_on(async {
            let mut pending = tools.execute_async(&call);
            let mut started = Box::pin(view.started.notified());
            std::future::poll_fn(|context| {
                assert!(
                    pending.as_mut().poll(context).is_pending(),
                    "resource wait must suspend the tool"
                );
                started.as_mut().poll(context)
            })
            .await;
            assert_eq!(
                fs::read_to_string(workspace.path().join("before.txt")).expect("waiting preimage"),
                "before\n"
            );
            assert!(
                !receipt_path(workspace.path()).exists(),
                "no effect receipt before durable checkpoint"
            );
            if disposition == "cancel" {
                view.cancelled.store(true, Ordering::Release);
            }
            if disposition == "revoke" {
                view.writable.store(false, Ordering::Release);
            }
            if disposition == "drift" {
                fs::write(workspace.path().join("before.txt"), "external change\n")
                    .expect("external edit while waiting");
            }
            view.available.notify_one();
            pending.await
        });
        let observation = observation.expect("tool result");
        assert_eq!(view.calls.load(Ordering::Relaxed), 1);
        assert_eq!(observation.is_error, disposition != "resume");
        assert_eq!(
            fs::read_to_string(workspace.path().join("before.txt")).expect("file"),
            match disposition {
                "resume" => "after\n",
                "drift" => "external change\n",
                _ => "before\n",
            }
        );
        if disposition == "resume" {
            let mut reopened = writable_tools(workspace.path())
                .with_checkpoint_observer(Arc::new(|_| Ok(())))
                .with_checkpoint_view(view.clone());
            let replay = reopened.execute(&call).expect("exact receipt replay after reopen");
            assert!(!replay.is_error, "{}", wire(&replay));
            assert_eq!(view.calls.load(Ordering::Relaxed), 1);
        }
    }
}
