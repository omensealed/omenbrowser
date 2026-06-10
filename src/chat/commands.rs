#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientCommand {
    Me(String),
    Notice(String),
    Topic(String),
    CreateRoom { room: String, topic: Option<String> },
    Kick(String),
    Ban(String),
    Unban(String),
    Mute(String),
    Unmute(String),
    Role { target: String, role: String },
    Join(String),
    Part(Option<String>),
    Upload(String),
    Rooms,
    Who,
    DirectMessage(String),
    Help,
    Unknown(String),
}

pub fn parse_client_command(input: &str) -> Option<ClientCommand> {
    let input = input.trim();
    let command = input.strip_prefix('/')?;
    let (name, rest) = command.split_once(' ').unwrap_or((command, ""));
    let rest = rest.trim();
    Some(match name {
        "me" => ClientCommand::Me(rest.to_owned()),
        "notice" => ClientCommand::Notice(rest.to_owned()),
        "topic" => ClientCommand::Topic(rest.to_owned()),
        "create" | "create-room" | "mkroom" => {
            let (room, topic) = rest.split_once(' ').unwrap_or((rest, ""));
            ClientCommand::CreateRoom {
                room: room.to_owned(),
                topic: (!topic.trim().is_empty()).then(|| topic.trim().to_owned()),
            }
        }
        "kick" => ClientCommand::Kick(rest.to_owned()),
        "ban" => ClientCommand::Ban(rest.to_owned()),
        "unban" => ClientCommand::Unban(rest.to_owned()),
        "mute" => ClientCommand::Mute(rest.to_owned()),
        "unmute" => ClientCommand::Unmute(rest.to_owned()),
        "role" => {
            let (target, role) = rest.split_once(' ').unwrap_or((rest, ""));
            ClientCommand::Role {
                target: target.to_owned(),
                role: role.trim().to_owned(),
            }
        }
        "join" => ClientCommand::Join(rest.to_owned()),
        "part" => ClientCommand::Part((!rest.is_empty()).then(|| rest.to_owned())),
        "upload" | "file" => ClientCommand::Upload(rest.to_owned()),
        "rooms" => ClientCommand::Rooms,
        "who" => ClientCommand::Who,
        "dm" | "query" => ClientCommand::DirectMessage(rest.to_owned()),
        "help" => ClientCommand::Help,
        other => ClientCommand::Unknown(other.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_room_accepts_documented_alias() {
        assert_eq!(
            parse_client_command("/create-room help Ask OMEN questions"),
            Some(ClientCommand::CreateRoom {
                room: "help".into(),
                topic: Some("Ask OMEN questions".into()),
            })
        );
    }
}
