use super::*;
use peritus_agent::{
    DeveloperControlFlow, DeveloperModelRole, DeveloperRequestAdmission, DeveloperToolEffect,
};

#[derive(Default)]
struct ToolIdentity {
    boundaries: Mutex<Vec<(bool, DeveloperModelRole, String, u32)>>,
}

impl DeveloperInteraction for ToolIdentity {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        Ok(DeveloperInput {
            revision: 1,
            conversation: "Use the exact admitted tool".to_owned(),
            images: Vec::new(),
        })
    }

    fn prepare_request(
        &self,
        _: u64,
        _: &ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        Ok(DeveloperRequestAdmission::Accepted)
    }

    fn admit_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
        _: DeveloperToolEffect,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        self.boundaries.lock().expect("boundaries").push((
            false,
            role,
            invocation.to_owned(),
            sequence,
        ));
        Ok(DeveloperControlFlow::Continue)
    }

    fn complete_tool(
        &self,
        role: DeveloperModelRole,
        invocation: &str,
        sequence: u32,
    ) -> Result<DeveloperControlFlow, DeveloperLoopError> {
        self.boundaries.lock().expect("boundaries").push((
            true,
            role,
            invocation.to_owned(),
            sequence,
        ));
        Ok(DeveloperControlFlow::Continue)
    }

    fn observe(&self, _: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        Ok(())
    }
}

#[test]
fn developer_loop_passes_its_stable_request_prefix_to_both_tool_boundaries() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([tool_response(), text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let port = ToolIdentity::default();
        DeveloperLoop::run_interactive(
            &provider,
            request(CancellationToken::new()),
            &mut RecordingTool::default(),
            &mut RecordingTrace::default(),
            None,
            &port,
        )
        .await
        .expect("interactive tool run");

        assert_eq!(
            *port.boundaries.lock().expect("boundaries"),
            [
                (false, DeveloperModelRole::Writer, "interactive-boundary-test".to_owned(), 1,),
                (true, DeveloperModelRole::Writer, "interactive-boundary-test".to_owned(), 1,),
            ]
        );
    });
}
