use serde::{Deserialize, Serialize};
use crate::domain::{Invitation, InvitationManager};
use crate::errors::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriageAction {
    Accept,
    Ignore,
    Skip,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TriageResult {
    pub total_reviewed: usize,
    pub accepted: usize,
    pub ignored: usize,
    pub skipped: usize,
    pub quit_early: bool,
}

pub trait TriageReader: Send {
    fn read_action(&mut self, invite: &Invitation) -> Result<TriageAction, AppError>;
}

/// Executes triage loop against an invitation queue using any TriageReader implementation
pub async fn triage_invitations<R: TriageReader, M: InvitationManager>(
    queue: &[Invitation],
    reader: &mut R,
    manager: &M,
    dry_run: bool,
) -> Result<TriageResult, AppError> {
    let mut result = TriageResult::default();

    for invite in queue {
        let action = reader.read_action(invite)?;
        match action {
            TriageAction::Accept => {
                let target = if invite.public_identifier.is_empty() {
                    invite.name.as_deref().unwrap_or("unknown")
                } else {
                    &invite.public_identifier
                };
                if !dry_run {
                    manager.accept_invitation(target).await?;
                }
                result.accepted += 1;
                result.total_reviewed += 1;
            }
            TriageAction::Ignore => {
                let target = if invite.public_identifier.is_empty() {
                    invite.name.as_deref().unwrap_or("unknown")
                } else {
                    &invite.public_identifier
                };
                if !dry_run {
                    manager.ignore_invitation(target).await?;
                }
                result.ignored += 1;
                result.total_reviewed += 1;
            }
            TriageAction::Skip => {
                result.skipped += 1;
                result.total_reviewed += 1;
            }
            TriageAction::Quit => {
                result.quit_early = true;
                break;
            }
        }
    }

    Ok(result)
}
