use std::sync::{Arc, Mutex};

use perfboard_core::{BoardSize, ComponentDefinition, ProjectId};
use perfboard_project::{
    Board, OperationError, ProjectDocument, ProjectSession, RevisionError, SessionError,
};
use perfboard_session_ipc::{
    RUNTIME_DIRECTORY_ENV, SessionApiError, SessionRegistration, SessionRequest,
    SessionRequestHandler, SessionResponse,
};
use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use tokio::process::Command;
use uuid::Uuid;

struct TestHandler {
    session: Mutex<ProjectSession>,
    component_library: Vec<ComponentDefinition>,
}

impl SessionRequestHandler for TestHandler {
    fn handle(&self, request: SessionRequest) -> Result<SessionResponse, SessionApiError> {
        let mut session = self.session.lock().unwrap();
        match request {
            SessionRequest::Ping => Ok(SessionResponse::Pong {
                revision: session.document().revision,
                project_name: session.document().name.clone(),
            }),
            SessionRequest::GetProject => Ok(SessionResponse::Project(Box::new(
                session.document().clone(),
            ))),
            SessionRequest::GetComponentLibrary => {
                Ok(SessionResponse::Components(self.component_library.clone()))
            }
            SessionRequest::ApplyTransaction(request) => session
                .apply_transaction(request)
                .map(|(result, _)| SessionResponse::Transaction(result))
                .map_err(|error| session_error(&error)),
            _ => Err(SessionApiError::new(
                "test.unsupported",
                "request is not implemented by fixture",
            )),
        }
    }
}

fn session_error(error: &SessionError) -> SessionApiError {
    let revisions = match &error {
        SessionError::Operation(OperationError::StaleRevision { expected, actual })
        | SessionError::Revision(RevisionError::StaleRevision { expected, actual }) => {
            (Some(*expected), Some(*actual))
        }
        _ => (None, None),
    };
    SessionApiError {
        code: if revisions.0.is_some() {
            "revision.stale".into()
        } else {
            "session.operation_failed".into()
        },
        message: error.to_string(),
        expected_revision: revisions.0,
        actual_revision: revisions.1,
        retryable: revisions.0.is_some(),
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keep the end-to-end MCP exchange readable in protocol order.
async fn real_stdio_server_reads_and_mutates_the_shared_session() {
    let directory = std::env::temp_dir().join(format!("perfboard-mcp-e2e-{}", Uuid::new_v4()));
    let document = ProjectDocument::new(
        ProjectId::new(),
        "MCP fixture",
        Board::new("board", BoardSize::new(24, 8).unwrap(), 2.54),
    );
    let definition = ComponentDefinition::from_toml(include_str!(
        "../../../components/rs485-breakout/component.toml"
    ))
    .unwrap();
    let handler = Arc::new(TestHandler {
        session: Mutex::new(ProjectSession::new(document)),
        component_library: vec![definition.clone()],
    });
    let (registration, listener) = SessionRegistration::bind_in(&directory).unwrap();
    let host = tokio::spawn(perfboard_session_ipc::serve(
        listener,
        Arc::clone(&registration),
        handler,
    ));

    let transport = TokioChildProcess::new(
        Command::new(env!("CARGO_BIN_EXE_perfboard-mcp-server")).configure(|command| {
            command.env(RUNTIME_DIRECTORY_ENV, &directory);
        }),
    )
    .unwrap();
    let client = ().serve(transport).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|tool| tool.name == "apply_operations"));

    let project = client
        .call_tool(CallToolRequestParams::new("get_project"))
        .await
        .unwrap();
    assert_eq!(project.is_error, Some(false));
    assert_eq!(
        project.structured_content.as_ref().unwrap()["data"]["value"]["revision"],
        0
    );

    let component_id = "00000000-0000-4000-8000-000000000101";
    let arguments = serde_json::from_value(json!({
        "expected_revision": 0,
        "rationale": "MCP component placement smoke test",
        "operations": [{
            "type": "add_component",
            "definition": definition,
            "instance": {
                "id": component_id,
                "definition_id": "dev.perfboard.rs485-breakout",
                "name": "MCP smoke-test RS-485 breakout",
                "position": { "x": 1, "y": 1 },
                "rotation": "deg0",
                "properties": {},
                "notes": "Placed through the real MCP stdio server"
            }
        }]
    }))
    .unwrap();
    let changed = client
        .call_tool(CallToolRequestParams::new("apply_operations").with_arguments(arguments))
        .await
        .unwrap();
    assert_eq!(changed.is_error, Some(false), "{changed:?}");
    assert_eq!(
        changed.structured_content.as_ref().unwrap()["data"]["value"]["revision"],
        1
    );

    let updated = client
        .call_tool(CallToolRequestParams::new("get_project"))
        .await
        .unwrap();
    assert_eq!(
        updated.structured_content.as_ref().unwrap()["data"]["value"]["design"]["components"]
            [component_id]["name"],
        "MCP smoke-test RS-485 breakout"
    );

    let stale_arguments = serde_json::from_value(json!({
        "expected_revision": 0,
        "rationale": "This races the previous request",
        "operations": [{
            "type": "move_component",
            "instance_id": component_id,
            "position": { "x": 2, "y": 1 }
        }]
    }))
    .unwrap();
    let stale = client
        .call_tool(CallToolRequestParams::new("apply_operations").with_arguments(stale_arguments))
        .await
        .unwrap();
    assert_eq!(stale.is_error, Some(true));
    assert_eq!(
        stale.structured_content.as_ref().unwrap()["error"]["code"],
        "revision.stale"
    );
    assert_eq!(
        stale.structured_content.as_ref().unwrap()["error"]["actual_revision"],
        1
    );

    client.cancel().await.unwrap();
    host.abort();
    drop(registration);
    std::fs::remove_dir_all(directory).unwrap();
}
