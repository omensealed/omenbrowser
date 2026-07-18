use std::path::PathBuf;

use omenbrowser_rs::chat::model::{ChatEvent, ChatEventKind, ChatRoomSummary, ChatServerSummary};
use omenbrowser_rs::chat::store::{ChatStore, SqliteChatStore};

const SERVER_ID: &str = "mixed-history-server";
const ROOM_ID: u32 = 7;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let stage = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing probe stage"))?;
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("missing isolated probe root"))?,
    );
    if args.next().is_some() {
        anyhow::bail!("unexpected mixed-history probe argument");
    }
    if root.as_os_str().is_empty()
        || root.exists() && root.symlink_metadata()?.file_type().is_symlink()
    {
        anyhow::bail!("mixed-history probe root must be an explicit non-symlink path");
    }
    std::fs::create_dir_all(&root)?;
    let database = root.join("chat.sqlite");
    let (expected_ids, append_id) = match stage.as_str() {
        "seed-old" => (Vec::new(), Some(100)),
        "reopen-current" => (vec![100], Some(101)),
        "reopen-old" => (vec![100, 101], Some(102)),
        "final-current" => (vec![100, 101, 102], None),
        _ => anyhow::bail!("unsupported mixed-history probe stage"),
    };

    let mut store = SqliteChatStore::open(&database)?;
    if stage == "seed-old" {
        store.save_server(ChatServerSummary {
            server_id: SERVER_ID.into(),
            destination: "00112233445566778899aabbccddeeff".into(),
            display_name: "Mixed history fixture".into(),
        })?;
        store.save_room(ChatRoomSummary {
            server_id: SERVER_ID.into(),
            room_id: ROOM_ID,
            name: "interop".into(),
            topic: Some("mixed-version persistence".into()),
            unread: 0,
            joined: true,
        })?;
        store.set_active_room(&SERVER_ID.to_string(), ROOM_ID)?;
    }

    verify_metadata(&store)?;
    verify_events(&store, expected_ids.as_slice())?;
    if let Some(event_id) = append_id {
        store.append_events(vec![fixture_event(event_id)])?;
        let mut after = expected_ids;
        after.push(event_id);
        verify_events(&store, after.as_slice())?;
    }

    println!(
        "{{\"status\":\"pass\",\"stage\":\"{}\",\"events\":{},\"metadata_verified\":true}}",
        stage,
        expected_ids_len(stage.as_str())
    );
    Ok(())
}

fn expected_ids_len(stage: &str) -> usize {
    match stage {
        "seed-old" => 1,
        "reopen-current" => 2,
        "reopen-old" | "final-current" => 3,
        _ => 0,
    }
}

fn verify_metadata(store: &SqliteChatStore) -> anyhow::Result<()> {
    let servers = store.saved_servers()?;
    if servers.len() != 1
        || servers[0].server_id != SERVER_ID
        || servers[0].display_name != "Mixed history fixture"
    {
        anyhow::bail!("mixed-history server metadata did not reopen exactly");
    }
    let rooms = store.rooms_for_server(&SERVER_ID.to_string())?;
    if rooms.len() != 1
        || rooms[0].room_id != ROOM_ID
        || rooms[0].name != "interop"
        || rooms[0].topic.as_deref() != Some("mixed-version persistence")
        || !rooms[0].joined
    {
        anyhow::bail!("mixed-history room metadata did not reopen exactly");
    }
    if store.active_room_id(&SERVER_ID.to_string())? != Some(ROOM_ID) {
        anyhow::bail!("mixed-history active room did not reopen exactly");
    }
    Ok(())
}

fn verify_events(store: &SqliteChatStore, expected_ids: &[u64]) -> anyhow::Result<()> {
    let events = store.latest_events(&SERVER_ID.to_string(), ROOM_ID, 16)?;
    let ids = events
        .iter()
        .map(|event| event.event_id)
        .collect::<Vec<_>>();
    if ids != expected_ids {
        anyhow::bail!("mixed-history event identifiers did not reopen exactly");
    }
    for event in events {
        let ChatEventKind::Message { body } = event.kind else {
            anyhow::bail!("mixed-history event kind changed during reopen");
        };
        if body != format!("mixed persisted event {}", event.event_id) {
            anyhow::bail!("mixed-history event body changed during reopen");
        }
    }
    Ok(())
}

fn fixture_event(event_id: u64) -> ChatEvent {
    ChatEvent {
        server_id: SERVER_ID.into(),
        room_id: ROOM_ID,
        event_id,
        actor_user_id: Some(42),
        actor_display_name: Some("Fixture user".into()),
        at_unix: 1_700_000_000 + event_id as i64,
        kind: ChatEventKind::Message {
            body: format!("mixed persisted event {event_id}"),
        },
    }
}
