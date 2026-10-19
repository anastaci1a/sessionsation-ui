//! User-facing descriptions; raw backend codes remain available to control logic.
pub fn describe(text: &str) -> String {
    for prefix in ["Launch failed: ", "Status: "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return format!("{prefix}{}", describe(rest));
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
            "Waiting for the backend to record this launch. Checking the original request; it will not be sent again."
        }
        "SESSION_NOT_FOUND" => {
            "This session is no longer in the current list. Return to Manage Sessions to refresh it."
        }
        "SESSION_UNAVAILABLE" => {
            "The session's process or saved history could not be located. Refresh the session list before trying again."
        }
        "SESSION_SOURCE_CHANGED" => {
            "The saved session changed. Select it again to review the current account, history and directory."
        }
        "SESSION_ALREADY_RUNNING" | "SESSION_ALREADY_RUNNING_OR_UNAVAILABLE" => {
            "The saved session is now running or no longer available to resume. Review it in Manage Sessions."
        }
        "SESSION_OPERATION_IN_PROGRESS" | "RESUME_IN_PROGRESS" | "OPERATION_BUSY" => {
            "Another action is still changing this session. Check its progress in Operations first."
        }
        "ACCOUNT_UNAVAILABLE" => {
            "The selected account is unavailable. Check that it is enabled and signed in."
        }
        "SAVED_DIRECTORY_UNAVAILABLE" => {
            "The saved directory is missing. Choose an available directory before resuming."
        }
        "SESSION_NAME_EXISTS" => "Another session already uses this name. Choose a different name.",
        "WORKSPACE_NOT_FOUND" => "The selected Space no longer exists. Choose another Space.",
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
        _ => return text.to_owned(),
    };
    message.into()
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
        assert!(describe("RECEIPT_NOT_FOUND").contains("not confirmed"));
    }
}
