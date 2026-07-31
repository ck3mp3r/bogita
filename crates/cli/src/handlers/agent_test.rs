use crate::args::AgentCommands;
use crate::handlers::agent::{handle_agent, AgentOutput};

#[tokio::test]
async fn agent_start_default_socket() {
    let output = handle_agent(AgentCommands::Start { socket: None })
        .await
        .unwrap();
    let AgentOutput::Started { socket } = output else {
        panic!("expected Started variant");
    };
    assert!(socket.contains("bogita-agent.sock"));
}

#[tokio::test]
async fn agent_start_custom_socket() {
    let output = handle_agent(AgentCommands::Start {
        socket: Some("/tmp/my.sock".to_string()),
    })
    .await
    .unwrap();
    let AgentOutput::Started { socket } = output else {
        panic!("expected Started variant");
    };
    assert_eq!(socket, "/tmp/my.sock");
}

#[tokio::test]
async fn agent_stop_returns_stopped() {
    let output = handle_agent(AgentCommands::Stop).await.unwrap();
    assert!(matches!(output, AgentOutput::Stopped));
}

#[tokio::test]
async fn agent_status_returns_not_running() {
    let output = handle_agent(AgentCommands::Status).await.unwrap();
    let AgentOutput::Status { running, socket } = output else {
        panic!("expected Status variant");
    };
    assert!(!running);
    assert!(socket.is_none());
}

#[tokio::test]
async fn agent_keys_returns_empty() {
    let output = handle_agent(AgentCommands::Keys).await.unwrap();
    let AgentOutput::Keys(keys) = output else {
        panic!("expected Keys variant");
    };
    assert!(keys.is_empty());
}
