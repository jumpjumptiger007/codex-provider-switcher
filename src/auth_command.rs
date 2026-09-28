use crate::{credential::CPS_KEYCHAIN_SERVICE, provider::CredentialSlotId};

pub const SECURITY_EXECUTABLE: &str = "/usr/bin/security";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCommand {
    pub command: &'static str,
    pub args: Vec<String>,
    pub refresh_interval_ms: u64,
}

/// Project a credential slot into the command Codex uses to retrieve its
/// secret. The secret is stored in Keychain and is never part of this value.
pub fn project_keychain_auth_command(slot: &CredentialSlotId) -> AuthCommand {
    AuthCommand {
        command: SECURITY_EXECUTABLE,
        args: [
            "find-generic-password",
            "-s",
            CPS_KEYCHAIN_SERVICE,
            "-a",
            slot.as_str(),
            "-w",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        refresh_interval_ms: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{SECURITY_EXECUTABLE, project_keychain_auth_command};
    use crate::credential::CPS_KEYCHAIN_SERVICE;
    use crate::provider::CredentialSlotId;

    #[test]
    fn command_uses_only_the_cps_service_and_requested_slot() {
        let slot = CredentialSlotId::new("openrouter").unwrap();
        let command = project_keychain_auth_command(&slot);

        assert_eq!(command.command, SECURITY_EXECUTABLE);
        assert_eq!(
            command.args,
            [
                "find-generic-password",
                "-s",
                CPS_KEYCHAIN_SERVICE,
                "-a",
                "openrouter",
                "-w"
            ]
        );
        assert_eq!(command.refresh_interval_ms, 0);
    }
}
