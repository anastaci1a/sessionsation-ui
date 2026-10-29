//! User-facing descriptions; raw backend codes remain available to control logic.
pub fn describe(text: &str) -> String {
    for prefix in ["Launch failed: ", "Status: "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let message = describe(rest);
            return if message.is_empty() {
                message
            } else {
                format!("{prefix}{message}")
            };
        }
    }
    let code = text.split([':', ' ', '\n']).next().unwrap_or("");
    let message = match code {
        "STALE_TARGET" => {
            "This session changed while the menu was open. Review its current state and choose the action again."
        }
        "SESSION_STATE_UNVERIFIED" => {
            "The session's current state is still being checked. No new action has been confirmed."
        }
        "RESUME_NOT_FOUND" | "LAUNCH_NOT_FOUND" => {
            "The backend has not returned this launch record yet. Its outcome is unconfirmed; it will not be sent again."
        }
        "SESSION_NOT_FOUND" => {
            "This session is no longer in the current list. Return to Manage sessions to refresh it."
        }
        "SESSION_NOT_ATTACHED" | "SESSION_ATTACHMENT_UNAVAILABLE" => {
            "This session no longer has a verified live terminal attachment. Refresh Manage sessions; use Start session if the agent has exited."
        }
        "SESSION_UNAVAILABLE" => {
            "The session's process or saved history could not be located. Refresh the session list before trying again."
        }
        "SESSION_SOURCE_CHANGED" => {
            "The saved session changed. Select it again to review the current account, history and directory."
        }
        "SESSION_ALREADY_RUNNING" | "SESSION_ALREADY_RUNNING_OR_UNAVAILABLE" => {
            "The saved session is now running or no longer available to resume. Review it in Manage sessions."
        }
        "SESSION_OPERATION_IN_PROGRESS" | "RESUME_IN_PROGRESS" | "OPERATION_BUSY" => {
            "Another action is still changing this session. Check its progress in Operations first."
        }
        "SESSION_STALE" => {
            "This session's process is no longer verified. Refresh Manage sessions; nothing was started or resumed."
        }
        "UNSUPPORTED_PROVIDER_OPERATION" => {
            "This provider does not support that action. No change was made."
        }
        "ACCOUNT_NAME_EXISTS" => {
            "An account already uses this name. Account names are shared by all providers; choose another."
        }
        "ACCOUNT_PROVIDER_MISMATCH" | "ACCOUNT_PROVIDER_REQUIRED" => {
            "The account's provider does not match the request. Choose the account's provider and try again."
        }
        "SESSION_NOT_IN_HERDR" => "Only a running HERDR session can be moved between Spaces.",
        "SPACE_MOVE_LOCATION_CONFLICT" | "SPACE_MOVE_IDENTITY_MISMATCH" => {
            "Terminal placement is not verified. Inspect the original operation in Operations before another move."
        }
        "ACCOUNT_UNAVAILABLE" => {
            "The selected account is unavailable. Check that it is enabled and signed in."
        }
        "SAVED_DIRECTORY_UNAVAILABLE" => {
            "The saved directory is missing. Choose an available directory before resuming."
        }
        "SESSION_NAME_EXISTS" => "Another session already uses this name. Choose a different name.",
        "WORKSPACE_NOT_FOUND" | "SPACE_NOT_FOUND" => {
            "The selected Space no longer exists. Choose another Space."
        }
        "WORKSPACE_NAME_EXISTS" => {
            "A Space already uses this name. Select that Space or choose another name."
        }
        "UNFINISHED_HISTORY_CONFIRMATION_REQUIRED" => {
            "This conversation ended during a turn. Review the unfinished-conversation warning before starting it."
        }
        "BACKEND_TIMEOUT" => {
            "The backend has not replied yet. Check Operations before repeating an action; a timeout does not mean it was rejected."
        }
        "RECEIPT_NOT_FOUND" => {
            "No delivery receipt was found for this request. Delivery is not confirmed; do not resend automatically."
        }
        "ADAPTER_BUSY" => {
            "The session connection is busy. Delivery is not confirmed; check the original request before sending again."
        }
        "HISTORY_CHANGED" | "SOURCE_HISTORY_CHANGED" => {
            "The saved conversation changed during verification. Its history has been retained; review it before trying again."
        }
        "SESSION_ACTIVE" => {
            "This conversation is still active or has a pending action. End it and wait for Operations to finish first."
        }
        "PERMISSION_DENIED" => {
            "Local data could not be accessed with the current permissions. Check the selected directory or data-root permissions."
        }
        "DIRECTORY_ALREADY_EXISTS" => "",
        "DIRECTORY_CONFIRMATION_REQUIRED" | "DIRECTORY_CHANGED" | "DIRECTORY_PARENT_CHANGED" => {
            "The selected directory changed or is unavailable. Choose it again before starting."
        }
        "PAGINATED_HISTORY_SERVER_REQUIRED" | "HISTORY_SERVER_REQUIRED" => {
            "Codex's history server is needed to preserve this conversation completely. Open the original home’s server before trying again."
        }
        "admitted" => "Launch accepted; preparing its terminal.",
        "preparing" => "Checking the session and preparing its destination.",
        "creating_terminal" => "Creating the new terminal tab.",
        "dispatching" | "starting_provider" => {
            "Starting the agent with the selected account and conversation."
        }
        "attached" => "The agent is attached; checking its process identity.",
        "transferring" => "Transferring conversation history to the selected account.",
        "checking_history" | "verifying_history" => "Verifying conversation history.",
        "compressing_history" => "Compressing conversation history into Archived Sessions.",
        "moving_history" => "Preserving archived history and clearing its original local copies.",
        "waiting_for_idle" => "Waiting for the agent to finish its current work safely.",
        "complete" => "Complete.",
        "uncertain" => {
            "The action's outcome is not confirmed. Check the original operation before repeating it."
        }
        "failed" => "The action could not finish. Review the details below.",
        _ => {
            if !code.is_empty()
                && code.contains('_')
                && code
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            {
                if let Some((_, detail)) = text.split_once(": ")
                    && detail != code
                {
                    return describe(detail);
                }
                return "The backend could not complete this action. Review Operations for details before trying again.".into();
            }
            return text.to_owned();
        }
    };
    message.into()
}
/// Phase labels are presentation only; reducers continue matching raw API values.
pub fn phase(value: &str) -> String {
    if matches!(value, "" | "unknown") {
        return "Checking operation state".into();
    }
    let described = describe(value);
    if described != value {
        return described;
    }
    let words = value.replace('_', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}
pub fn operation_phase(kind: &str, value: &str) -> String {
    if kind == "move_space" {
        return match value {
            "admitted" => "Preparing the terminal move.",
            "dispatching" => "Moving the existing terminal; its agent keeps running.",
            "reconciling" => "Verifying the moved terminal's ownership.",
            _ => return phase(value),
        }
        .into();
    }
    phase(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptions_distinguish_pending_receipts_from_rejected_actions() {
        assert!(describe("RESUME_NOT_FOUND: RESUME_NOT_FOUND").contains("will not be sent again"));
        assert!(describe("STALE_TARGET").contains("choose the action again"));
        assert!(describe("Launch failed: ACCOUNT_UNAVAILABLE").contains("enabled and signed in"));
        assert_eq!(describe("History retained."), "History retained.");
        assert!(!describe("UNRECOGNIZED_FAILURE").contains("UNRECOGNIZED_FAILURE"));
        assert_eq!(
            describe("OPERATION_FAILED: retained explanatory detail"),
            "retained explanatory detail"
        );
        assert!(describe("RECEIPT_NOT_FOUND").contains("not confirmed"));
        assert_eq!(
            phase("awaiting_risk_confirmation"),
            "Awaiting risk confirmation"
        );
        assert_eq!(phase("unknown"), "Checking operation state");
        assert!(operation_phase("move_space", "dispatching").contains("keeps running"));
        assert!(!operation_phase("move_space", "admitted").contains("Launch"));
    }
}
