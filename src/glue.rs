//! The game-side glue: the sapling's Part, the acorn's Part, the growth tick, the world
//! record and the `/dynamictrees` readout.
//!
//! Every rule is [`crate::runtime`]'s; this module reads blocks, calls it and writes the
//! result back. The record is one file in the save's mod-data folder, so a relaunch resumes a
//! growing tree exactly where it stopped.

use std::sync::{Mutex, MutexGuard};

use mod_api::{
    BlockPart, BlockPos, BlockUpdateFlags, CanSurviveEvent, ConfigPath, DimensionKey, Flow, Host,
    HostSide, HostTransaction, InteractionResult, IsBonemealSuccessEvent,
    IsValidBonemealTargetEvent, ItemPart, ItemPlace, ItemResource, LevelDimension, LevelHost,
    LevelRef, LevelSide, LevelWriteHost, PerformBonemealEvent, RandomTickEvent, Registrar,
    ServerHost, Text, UseOnEvent, WorldPhase,
};

use crate::runtime::{self, Step, StepKind, Tree};

/// The record's name inside `<save>/horizon-mod-data/dynamictrees/`.
const RECORD: &str = "trees.tsv";
/// How many trees `/dynamictrees list` prints.
const LIST_LIMIT: usize = 10;

/// The write flags every placement uses: tell the neighbours and send it to the clients, as
/// a player's own placement does (`Block.UPDATE_ALL` without the neighbour-shape skip).
fn place_flags() -> BlockUpdateFlags {
    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS
}

/// The session state: the loaded record and the pulse clock. `Hooks: Send + Sync`, so it is a
/// `Mutex`, and the macro boxes one `Default` instance for the session.
#[derive(Default)]
struct Core {
    /// The record has been read from the save (there is no world-open hook at this pin, so it
    /// is read at the first running tick).
    loaded: bool,
    /// Ticks since the record was loaded, for the pulse cadence.
    ticks: u64,
    /// Every growing or finished tree, in the order it was planted.
    trees: Vec<Tree>,
    /// The record differs from the file.
    dirty: bool,
    /// The last record write's failure, shown by `/dynamictrees list`.
    error: Option<String>,
}

static CORE: Mutex<Option<Core>> = Mutex::new(None);

fn core() -> MutexGuard<'static, Option<Core>> {
    CORE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

/// Attach the sapling's Part to the sapling block and the acorn's Part to the seed item.
pub(crate) fn register(r: &mut dyn Registrar) {
    let sapling = r.block_key(runtime::OAK_SAPLING);
    r.block_part(sapling, SaplingPart);
    let seed = r.item_key(runtime::OAK_SEED);
    r.item_part(seed, SeedPart);
}

// ---------------------------------------------------------------------------
// The sapling
// ---------------------------------------------------------------------------

/// The sapling's behaviour: upstream's `DynamicSaplingBlock`, on the `minecraft:poppy` template
/// (a plant on soil with no growth and no bone meal of its own — see `notes.md`).
struct SaplingPart;

#[mod_api::horizon_part]
impl BlockPart for SaplingPart {
    /// Upstream's `randomTick`: grow into the tree when the sapling can, else leave the block
    /// to its template. `Flow::Cancel` sets the hook's `handled`, so a sprouted sapling does
    /// not also take its template's random tick.
    fn random_tick(&self, level: &mut dyn LevelWriteHost, event: &mut RandomTickEvent) -> Flow {
        if sprout(level, event.dimension, pos_of(event.pos)) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// Upstream's `canSaplingStay`: an acceptable soil below, air above, and no branch or
    /// sapling beside it.
    fn can_survive(&self, level: &dyn LevelHost, event: &mut CanSurviveEvent) -> Option<bool> {
        let pos = pos_of(event.pos);
        let beside = neighbours(level, pos);
        let beside: Vec<&str> = beside.iter().map(String::as_str).collect();
        Some(runtime::can_stay(
            &block_id(level, [pos[0], pos[1] - 1, pos[2]]),
            &block_id(level, [pos[0], pos[1] + 1, pos[2]]),
            &beside,
        ))
    }

    /// Upstream's `isValidBonemealTarget`: a meal is only a target where the tree can actually
    /// grow, and only the server grows it (the client answers no, so it predicts nothing).
    fn is_valid_bonemeal_target(
        &self,
        level: LevelSide<'_>,
        event: &mut IsValidBonemealTargetEvent,
    ) -> Option<bool> {
        let (level, server): (&dyn LevelHost, bool) = match level {
            LevelSide::Server(level) => (level, true),
            LevelSide::Client(level) => (level, false),
        };
        let pos = pos_of(event.pos);
        let can = runtime::can_sprout(
            &block_id(level, [pos[0], pos[1] - 1, pos[2]]),
            &block_id(level, [pos[0], pos[1] + 1, pos[2]]),
        );
        Some(can && server)
    }

    /// Upstream's `isBonemealSuccess` is always true for a target sapling.
    fn is_bonemeal_success(
        &self,
        _level: &mut dyn mod_api::LevelRandomHost,
        event: &mut IsBonemealSuccessEvent,
    ) {
        event.success = true;
    }

    /// Upstream's `performBonemeal`: grow into the tree (the meal is spent either way).
    fn perform_bonemeal(&self, level: &mut dyn LevelWriteHost, event: &mut PerformBonemealEvent) {
        sprout(level, event.dimension, pos_of(event.pos));
    }
}

// ---------------------------------------------------------------------------
// The acorn
// ---------------------------------------------------------------------------

/// The acorn's behaviour: upstream's `Seed.onItemUsePlantSeed` — the top face only, the cell
/// above the clicked block, and the sapling's own placement rule.
struct SeedPart;

#[mod_api::horizon_part]
impl ItemPart for SeedPart {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        // The top face only: `[0, 1, 0]` is vanilla's `Direction.UP`.
        if event.face != [0, 1, 0] {
            return None;
        }
        let level = host.player(event.player)?.level(host)?;
        let soil = block_in(host, level, event.pos);
        let target = [event.pos[0], event.pos[1] + 1, event.pos[2]];
        let target_id = block_in(host, level, target);
        let above = block_in(host, level, [target[0], target[1] + 1, target[2]]);
        let beside = neighbours_in(host, level, target);
        let beside: Vec<&str> = beside.iter().map(String::as_str).collect();
        if !runtime::can_plant(&soil, &target_id, &above, &beside) {
            return None;
        }
        let state = host.block_state_id(runtime::OAK_SAPLING)?;
        if !matches!(
            level.set_block(host, target, state, place_flags()),
            Ok(true)
        ) {
            return None;
        }
        spend_one(host, event);
        Some(InteractionResult::SuccessServer)
    }
}

/// Take one acorn out of the clicking hand (`ItemPlace::PlayerHand` is the engine's own move,
/// so a creative hand gives it without losing it and the client is told).
fn spend_one(host: &mut dyn ServerHost, event: &UseOnEvent<'_>) {
    let resource = ItemResource::of(event.stack);
    let place = ItemPlace::PlayerHand {
        player: event.player,
        hand: event.hand,
    };
    let _ = host.transaction(|host, tx| Ok::<u32, ()>(place.extract(host, tx, &resource, 1)));
}

// ---------------------------------------------------------------------------
// Growing
// ---------------------------------------------------------------------------

/// Grow a sapling into the tree's first step (upstream's `transitionToTree`): the record is
/// created, the trunk base is written over the sapling, and the growth tick takes it from
/// there. `false` when the sapling cannot grow here.
fn sprout(level: &mut dyn LevelWriteHost, dimension: LevelDimension, pos: [i32; 3]) -> bool {
    let soil = block_id(level, [pos[0], pos[1] - 1, pos[2]]);
    let above = block_id(level, [pos[0], pos[1] + 1, pos[2]]);
    if !runtime::can_sprout(&soil, &above) {
        return false;
    }
    let dimension = dimension.key().id().to_owned();
    let mut guard = core();
    let state = guard.get_or_insert_with(Core::default);
    if state
        .trees
        .iter()
        .any(|tree| tree.base == pos && tree.dimension == dimension)
    {
        return false;
    }
    let mut tree = Tree::planted(dimension, pos, seed_for(pos));
    let Some(step) = tree.sprout() else {
        return false;
    };
    if !place(level, &step) {
        return false;
    }
    state.trees.push(tree);
    state.dirty = true;
    true
}

/// The seed a tree at `pos` grows from: its own coordinates mixed with the session's clock, so
/// two acorns planted at one spot in different worlds are not the same tree. Deterministic per
/// position, which is what makes a relaunch repeat the plan.
fn seed_for(pos: [i32; 3]) -> u64 {
    let mut seed = 0x5DEE_CE66_D1CE_B00D_u64;
    for value in pos {
        seed = runtime::mix(seed, u64::from(value.unsigned_abs()));
    }
    seed
}

/// Write one step's blocks. `false` when the level refused every write (an unloaded chunk).
fn place(level: &mut dyn LevelWriteHost, step: &Step) -> bool {
    let id = match step.kind {
        StepKind::Branch => runtime::OAK_BRANCH,
        StepKind::Leaf => runtime::OAK_LEAVES,
    };
    let Some(state) = level.block_state_id(id) else {
        return false;
    };
    let mut wrote = false;
    for cell in &step.blocks {
        wrote |= level.set_block(*cell, state, place_flags());
    }
    wrote
}

/// The end of a running server tick: load the record once, then one pulse per
/// [`runtime::PULSE_TICKS`] for every tree whose chunk is loaded, and flush the record when it
/// changed.
pub(crate) fn on_server_tick(host: &mut dyn ServerHost, running: bool) {
    if !running {
        return;
    }
    let mut guard = core();
    let state = guard.get_or_insert_with(Core::default);
    if !state.loaded {
        load(host, state);
        state.loaded = true;
    }
    state.ticks = state.ticks.saturating_add(1);
    if state.ticks.is_multiple_of(runtime::PULSE_TICKS) {
        pulse(host, state);
    }
    if state.dirty {
        save(host, state);
    }
}

/// One growth pulse for every tree. A tree whose base cell is gone (chopped, or a chunk that
/// unloaded and came back empty) is dropped from the record.
fn pulse(host: &mut dyn ServerHost, state: &mut Core) {
    let mut changed = false;
    let mut removed = false;
    let mut index = 0;
    while index < state.trees.len() {
        let (dimension, base, placed) = {
            let tree = &state.trees[index];
            (tree.dimension.clone(), tree.base, tree.steps_done)
        };
        let Some(level) = level_named(host, &dimension) else {
            index += 1;
            continue;
        };
        let Some(base_block) = level.block(host, base) else {
            // The chunk is not loaded: leave the tree where it is.
            index += 1;
            continue;
        };
        let base_id = base_block.id(host);
        if placed > 0 && base_id != runtime::OAK_BRANCH {
            state.trees.remove(index);
            removed = true;
            continue;
        }
        let step = state.trees[index].pulse();
        if let Some(step) = step
            && place_in(host, level, &step)
        {
            changed = true;
        }
        index += 1;
    }
    if changed || removed {
        state.dirty = true;
    }
}

/// Write one step's blocks through a `LevelRef` (the tick's own path).
fn place_in(host: &mut dyn ServerHost, level: LevelRef, step: &Step) -> bool {
    let id = match step.kind {
        StepKind::Branch => runtime::OAK_BRANCH,
        StepKind::Leaf => runtime::OAK_LEAVES,
    };
    let Some(state) = host.block_state_id(id) else {
        return false;
    };
    let mut wrote = false;
    for cell in &step.blocks {
        wrote |= matches!(level.set_block(host, *cell, state, place_flags()), Ok(true));
    }
    wrote
}

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

/// Read the record from the save's mod-data folder. A missing file is an empty world.
fn load(host: &dyn ServerHost, state: &mut Core) {
    let Ok(path) = ConfigPath::new(RECORD) else {
        return;
    };
    let Ok(bytes) = host.world_data_files().read(&path) else {
        state.trees.clear();
        return;
    };
    let text = String::from_utf8_lossy(&bytes);
    state.trees = text.lines().filter_map(Tree::from_row).collect();
}

/// Write the record. A failure is kept for `/dynamictrees list` rather than swallowed.
fn save(host: &dyn ServerHost, state: &mut Core) {
    let Ok(path) = ConfigPath::new(RECORD) else {
        return;
    };
    let mut text = String::new();
    for tree in &state.trees {
        text.push_str(&tree.to_row());
        text.push('\n');
    }
    match host.world_data_files().write(&path, text.as_bytes()) {
        Ok(()) => {
            state.dirty = false;
            state.error = None;
        }
        Err(error) => state.error = Some(error.to_string()),
    }
}

/// Flush the record when the server stops (the worlds are still open there).
pub(crate) fn on_server_stopping(host: &mut dyn ServerHost) {
    let mut guard = core();
    if let Some(state) = guard.as_mut()
        && (state.dirty || state.error.is_some())
    {
        save(host, state);
    }
}

/// A world closed: write what is left, then forget it. Without this a session that closes one
/// world and opens another would carry the first world's trees into the second (the record is
/// per save, and it is read again at the new world's first running tick).
pub(crate) fn on_world_lifecycle(host: &mut dyn ServerHost, phase: WorldPhase) {
    match phase {
        WorldPhase::WorldCloseFinal | WorldPhase::WorldCloseFailed => {
            let mut guard = core();
            if let Some(state) = guard.as_mut()
                && (state.dirty || state.error.is_some())
            {
                save(host, state);
            }
            *guard = None;
        }
        // There is no open phase at this pin: the record is read at the first running tick.
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// The readout
// ---------------------------------------------------------------------------

/// The `/dynamictrees` command's answer: `info <pos>` or `list`.
pub(crate) fn run_command(
    host: &mut dyn ServerHost,
    event: &mut mod_api::RunModCommandEvent<'_>,
) -> Flow {
    let literals: Vec<&str> = event.command.literals.iter().map(String::as_str).collect();
    if literals.first() != Some(&"dynamictrees") {
        return Flow::Continue;
    }
    let line = match (literals.get(1), block_pos(event.command.argument("pos"))) {
        (Some(&"info"), Some(pos)) => info_line(&*host, event.command.dimension, pos),
        (Some(&"list"), _) => list_lines(),
        _ => "usage: /dynamictrees info <x> <y> <z> | /dynamictrees list".to_owned(),
    };
    let _ = host.commands().feedback(Text::literal(line));
    Flow::Cancel
}

/// One tree's readout, as the harness reads it: the record's stage when the tree has one,
/// and `sapling` for a sapling that has not sprouted yet (it has no record until it does).
fn info_line(host: &dyn ServerHost, key: DimensionKey, pos: [i32; 3]) -> String {
    let guard = core();
    if let Some(tree) = guard
        .as_ref()
        .and_then(|state| state.trees.iter().find(|tree| tree.base == pos))
    {
        let plan = tree.plan();
        return format!(
            "dynamictrees: {} {} {} {} stage={} steps={}/{} blocks={} trunk={} branches={}",
            tree.dimension,
            tree.base[0],
            tree.base[1],
            tree.base[2],
            tree.stage().name(),
            tree.steps_done,
            plan.len(),
            tree.blocks_placed(),
            plan.trunk_height,
            plan.branch_count,
        );
    }
    // No record: a sapling that has not sprouted yet is still the first stage of a tree.
    if let Some(level) = level_for_key(host, key)
        && block_in(host, level, pos) == runtime::OAK_SAPLING
    {
        let plan = runtime::Plan::new(pos, seed_for(pos));
        return format!(
            "dynamictrees: {} {} {} {} stage=sapling steps=0/{} blocks=0 trunk={} branches={}",
            level.dimension_id(),
            pos[0],
            pos[1],
            pos[2],
            plan.len(),
            plan.trunk_height,
            plan.branch_count,
        );
    }
    format!("dynamictrees: no tree at {} {} {}", pos[0], pos[1], pos[2])
}

/// Every tree, one line each, then a count.
fn list_lines() -> String {
    let guard = core();
    let Some(state) = guard.as_ref() else {
        return "dynamictrees: 0 trees".to_owned();
    };
    let mut text = String::new();
    for tree in state.trees.iter().take(LIST_LIMIT) {
        text.push_str(&format!(
            "dynamictrees: {} {} {} {} stage={}\n",
            tree.dimension,
            tree.base[0],
            tree.base[1],
            tree.base[2],
            tree.stage().name()
        ));
    }
    text.push_str(&format!("dynamictrees: {} trees", state.trees.len()));
    if let Some(error) = &state.error {
        text.push_str(&format!(" (record error: {error})"));
    }
    text
}

// ---------------------------------------------------------------------------
// Small reads
// ---------------------------------------------------------------------------

/// A `BlockPos` as three coordinates.
fn pos_of(pos: BlockPos) -> [i32; 3] {
    let BlockPos { x, y, z } = pos;
    [x, y, z]
}

/// The block at `pos` as a registry name; an unreadable state is the empty string, which no
/// soil or plant matches.
fn block_id(level: &dyn LevelHost, pos: [i32; 3]) -> String {
    level
        .block_state(pos)
        .and_then(|state| level.state_block(state))
        .unwrap_or_default()
        .to_owned()
}

/// The block at `pos` of `level`, read through the host (the tick's own path).
fn block_in(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> String {
    level
        .block(host, pos)
        .map(|block| block.id(host))
        .unwrap_or_default()
}

/// The four horizontal neighbours of `pos`, read through the host.
fn neighbours_in(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> [String; 4] {
    [
        block_in(host, level, [pos[0] + 1, pos[1], pos[2]]),
        block_in(host, level, [pos[0] - 1, pos[1], pos[2]]),
        block_in(host, level, [pos[0], pos[1], pos[2] + 1]),
        block_in(host, level, [pos[0], pos[1], pos[2] - 1]),
    ]
}

/// The four horizontal neighbours of `pos`, read through the level a Part is given.
fn neighbours(level: &dyn LevelHost, pos: [i32; 3]) -> [String; 4] {
    [
        block_id(level, [pos[0] + 1, pos[1], pos[2]]),
        block_id(level, [pos[0] - 1, pos[1], pos[2]]),
        block_id(level, [pos[0], pos[1], pos[2] + 1]),
        block_id(level, [pos[0], pos[1], pos[2] - 1]),
    ]
}

/// The open level named `dimension`.
fn level_named(host: &dyn ServerHost, dimension: &str) -> Option<LevelRef> {
    host.levels()
        .list_levels()
        .into_iter()
        .find(|level| level.dimension_id() == dimension)
}

/// The open level whose key is `key`.
fn level_for_key(host: &dyn ServerHost, key: DimensionKey) -> Option<LevelRef> {
    host.levels()
        .list_levels()
        .into_iter()
        .find(|level| level.dimension() == key)
}

/// A command's `pos` argument.
pub(crate) fn block_pos(argument: Option<&mod_api::CommandArgumentValue>) -> Option<[i32; 3]> {
    match argument? {
        mod_api::CommandArgumentValue::BlockPos(pos) => Some(*pos),
        _ => None,
    }
}
