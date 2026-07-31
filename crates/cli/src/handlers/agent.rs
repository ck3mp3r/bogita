//! Agent CLI command handlers.
//!
//! Agent lifecycle (start/stop) and key listing are stubbed here.
//! Full SSH agent daemon implementation is a separate phase.

use crate::args::AgentCommands;
use bogita_core::error::Result;

pub enum AgentOutput {
    /// Agent started (or was already running): socket path.
    Started { socket: String },
    /// Agent stopped.
    Stopped,
    /// Agent status snapshot.
    Status {
        running: bool,
        socket: Option<String>,
    },
    /// Keys currently served by the agent.
    Keys(Vec<String>),
}

/// Dispatch `bogita agent <subcommand>`.
pub async fn handle_agent(cmd: AgentCommands) -> Result<AgentOutput> {
    match cmd {
        AgentCommands::Start { socket } => {
            let path = socket.unwrap_or_else(|| {
                std::env::temp_dir()
                    .join("bogita-agent.sock")
                    .to_string_lossy()
                    .to_string()
            });
            // TODO: daemonize SSH agent process (Phase 5)
            Ok(AgentOutput::Started { socket: path })
        }
        AgentCommands::Stop => {
            // TODO: signal running agent to shut down (Phase 5)
            Ok(AgentOutput::Stopped)
        }
        AgentCommands::Status => {
            // TODO: probe socket for liveness (Phase 5)
            Ok(AgentOutput::Status {
                running: false,
                socket: None,
            })
        }
        AgentCommands::Keys => {
            // TODO: query agent for loaded keys (Phase 5)
            Ok(AgentOutput::Keys(vec![]))
        }
    }
}
