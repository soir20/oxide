use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::OnceLock,
};

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};

use crate::game_server::PlayerSnapshot;
use crate::info;

static DATABASE: OnceLock<Mutex<Connection>> = OnceLock::new();

/// Opens (or creates) the database.  Call once at startup.
pub fn init(path: &Path) -> rusqlite::Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS players (
            guid                INTEGER PRIMARY KEY,
            first_name          TEXT    NOT NULL,
            last_name           TEXT    NOT NULL,
            credits             INTEGER NOT NULL,
            active_battle_class INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS owned_items (
            player_guid INTEGER NOT NULL REFERENCES players(guid) ON DELETE CASCADE,
            item_guid   INTEGER NOT NULL,
            PRIMARY KEY (player_guid, item_guid)
        );
        CREATE TABLE IF NOT EXISTS equipped_items (
            player_guid  INTEGER NOT NULL REFERENCES players(guid) ON DELETE CASCADE,
            battle_class INTEGER NOT NULL,
            slot         INTEGER NOT NULL,
            item_guid    INTEGER NOT NULL,
            PRIMARY KEY (player_guid, battle_class, slot)
        );
        ",
    )?;
    let _ = DATABASE.set(Mutex::new(conn));
    Ok(())
}

fn connection() -> &'static Mutex<Connection> {
    DATABASE.get().expect("persistence::init was not called")
}

pub fn load_player(guid: u32) -> Option<PlayerSnapshot> {
    match try_load_player(guid) {
        Ok(saved) => saved,
        Err(err) => {
            info!("Unable to load player {guid} from the database: {err}");
            None
        }
    }
}

fn try_load_player(guid: u32) -> rusqlite::Result<Option<PlayerSnapshot>> {
    let conn = connection().lock();

    let Some((first_name, last_name, credits, active_battle_class)) = conn
        .query_row(
            "SELECT first_name, last_name, credits, active_battle_class
             FROM players WHERE guid = ?1",
            params![guid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
    else {
        return Ok(None);
    };

    let mut owned_items = BTreeSet::new();
    let mut stmt = conn.prepare("SELECT item_guid FROM owned_items WHERE player_guid = ?1")?;
    for item in stmt.query_map(params![guid], |row| row.get::<_, i32>(0))? {
        owned_items.insert(item?);
    }

    let mut equipped: BTreeMap<u32, BTreeMap<u32, i32>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT battle_class, slot, item_guid FROM equipped_items WHERE player_guid = ?1",
    )?;
    for row in stmt.query_map(params![guid], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, i32>(2)?,
        ))
    })? {
        let (battle_class, slot, item_guid) = row?;
        equipped
            .entry(battle_class)
            .or_default()
            .insert(slot, item_guid);
    }

    Ok(Some(PlayerSnapshot {
        guid,
        first_name,
        last_name,
        credits,
        active_battle_class,
        owned_items,
        equipped,
    }))
}

pub fn save_players(snapshots: &[PlayerSnapshot]) {
    if snapshots.is_empty() {
        return;
    }
    match try_save_players(snapshots) {
        Ok(()) => info!("Saved {} player(s) to the database", snapshots.len()),
        Err(err) => info!("Unable to save players to the database: {err}"),
    }
}

fn try_save_players(snapshots: &[PlayerSnapshot]) -> rusqlite::Result<()> {
    let mut conn = connection().lock();
    let tx = conn.transaction()?;
    for player in snapshots {
        tx.execute(
            "INSERT INTO players (guid, first_name, last_name, credits, active_battle_class)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(guid) DO UPDATE SET
                 first_name = excluded.first_name,
                 last_name = excluded.last_name,
                 credits = excluded.credits,
                 active_battle_class = excluded.active_battle_class",
            params![
                player.guid,
                player.first_name,
                player.last_name,
                player.credits,
                player.active_battle_class
            ],
        )?;

        tx.execute(
            "DELETE FROM owned_items WHERE player_guid = ?1",
            params![player.guid],
        )?;
        for item_guid in &player.owned_items {
            tx.execute(
                "INSERT INTO owned_items (player_guid, item_guid) VALUES (?1, ?2)",
                params![player.guid, item_guid],
            )?;
        }

        tx.execute(
            "DELETE FROM equipped_items WHERE player_guid = ?1",
            params![player.guid],
        )?;
        for (battle_class, items) in &player.equipped {
            for (slot, item_guid) in items {
                tx.execute(
                    "INSERT INTO equipped_items (player_guid, battle_class, slot, item_guid)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![player.guid, battle_class, slot, item_guid],
                )?;
            }
        }
    }
    tx.commit()
}
