//! Typed downstream wire commands, frames and stable failure categories.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fault {
    InvalidRequest,
    InvalidInvitation,
    StorageCorrupt,
    CapacityExceeded,
    StorageConflict,
    StorageUnavailable,
    StorageOutcomeUnknown,
    NotFound,
    EnrollmentUnknown,
    EnrollmentRejected,
    ScopeExpired,
    NetworkUnavailable,
    ProtocolError,
    SourceUnavailable,
    NotAuthorized,
    InvalidImage,
    Cancelled,
    Timeout,
    Conflict,
    InvalidBoard,
    RenderError,
    BridgeTerminated,
}
impl Fault {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "InvalidRequest",
            Self::InvalidInvitation => "InvalidInvitation",
            Self::StorageCorrupt => "StorageCorrupt",
            Self::CapacityExceeded => "CapacityExceeded",
            Self::StorageConflict => "StorageConflict",
            Self::StorageUnavailable => "StorageUnavailable",
            Self::StorageOutcomeUnknown => "StorageOutcomeUnknown",
            Self::NotFound => "NotFound",
            Self::EnrollmentUnknown => "EnrollmentUnknown",
            Self::EnrollmentRejected => "EnrollmentRejected",
            Self::ScopeExpired => "ScopeExpired",
            Self::NetworkUnavailable => "NetworkUnavailable",
            Self::ProtocolError => "ProtocolError",
            Self::SourceUnavailable => "SourceUnavailable",
            Self::NotAuthorized => "NotAuthorized",
            Self::InvalidImage => "InvalidImage",
            Self::Cancelled => "Cancelled",
            Self::Timeout => "Timeout",
            Self::Conflict => "Conflict",
            Self::InvalidBoard => "InvalidBoard",
            Self::RenderError => "RenderError",
            Self::BridgeTerminated => "BridgeTerminated",
        }
    }
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::NotAuthorized
                | Self::ProtocolError
                | Self::CapacityExceeded
                | Self::InvalidInvitation
                | Self::NotFound
                | Self::StorageCorrupt
                | Self::StorageUnavailable
                | Self::StorageOutcomeUnknown
                | Self::RenderError
                | Self::BridgeTerminated
        )
    }
    pub fn retryable(self) -> bool {
        !self.terminal()
    }
    pub fn retryable_code(code: &str) -> bool {
        serde_json::from_value::<Self>(Value::String(code.into()))
            .unwrap_or(Self::NetworkUnavailable)
            .retryable()
    }
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for Fault {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Invite,
    Public,
    Owner,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permissions {
    pub interact: bool,
    pub edit_layout: bool,
}

/// JSON board/A2UI bodies remain opaque here; their materializers own those schemas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase", deny_unknown_fields)]
pub enum ControlCommand {
    GatewayPairStart {
        invitation: String,
        name: String,
    },
    GatewayPairStatus {
        id: String,
    },
    GatewayPairRetrySave {
        id: String,
    },
    BoardsList {},
    BoardGet {
        id: String,
    },
    BoardCreate {
        value: Value,
    },
    BoardUpdate {
        id: String,
        value: Value,
    },
    BoardDelete {
        id: String,
        value: Value,
    },
    Presets {},
    Connections {},
    Sharing {},
    SharesList {},
    ShareCreate {
        board_id: String,
        public: bool,
        permissions: Permissions,
    },
    ShareRevoke {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Rpc {
    Ping {},
    Control { command: ControlCommand },
    Action { lease: String, message: Value },
    Layout { value: Value },
    Image { lease: String, uri: String },
}
impl Rpc {
    pub fn image_lease(&self) -> Option<&str> {
        match self {
            Self::Image { lease, .. } => Some(lease),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Parse {
        invitation: String,
    },
    List {},
    Register {
        invitation: String,
        #[serde(default)]
        save: bool,
    },
    Forget {
        id: String,
    },
    Connect {
        id: String,
    },
    Disconnect {},
    Next {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ack: Option<String>,
    },
    Rpc {
        value: Rpc,
    },
}
impl Command {
    pub fn image_lease(&self) -> Option<&str> {
        match self {
            Self::Rpc { value } => value.image_lease(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Welcome {
    #[serde(rename = "welcome")]
    Shared {
        board: Value,
        permissions: Permissions,
    },
    #[serde(rename = "ownerWelcome")]
    Owner { services: Vec<Value> },
}
impl Welcome {
    pub fn permissions(&self, mode: Mode) -> Result<Permissions, Fault> {
        match (self, mode) {
            (Self::Owner { .. }, Mode::Owner) => Ok(Permissions {
                interact: true,
                edit_layout: true,
            }),
            (Self::Shared { permissions, .. }, Mode::Public)
                if permissions.interact || permissions.edit_layout =>
            {
                Err(Fault::ProtocolError)
            }
            (Self::Shared { permissions, .. }, Mode::Invite | Mode::Public) => Ok(*permissions),
            _ => Err(Fault::ProtocolError),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RenderFrame {
    Message {
        sequence: String,
        message: Value,
        lease: Option<String>,
    },
    SnapshotEnd {
        sequence: String,
    },
}
impl RenderFrame {
    pub fn sequence(&self) -> &str {
        match self {
            Self::Message { sequence, .. } | Self::SnapshotEnd { sequence } => sequence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Snapshot,
    Live,
    Closed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFrame {
    pub sequence: String,
    pub surface: Option<String>,
    pub lease: Option<String>,
    pub invalidated_leases: Vec<String>,
    pub snapshot_end: bool,
}
