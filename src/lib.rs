//! Dynamic Trees for Horizon — slice 1, the oak.
//!
//! A port of Dynamic Trees 1.7.2 (`DynamicTreesTeam/DynamicTrees`, branch `release/1.21.1`,
//! commit `8bf66d83…`, MIT) as one flat SDK package (`api = "0.2"`). The oak's acorn plants a
//! sapling on soil; the sapling's own random tick or a bone-meal click starts the tree; and
//! the tree then grows through named stages over time — sprout, trunk, branches, canopy — one
//! block per pulse, with upstream's `growth_rate 0.8` as a seeded gate.
//!
//! The moments this mod answers:
//!
//! - `register`: the sapling's block Part and the acorn's item Part.
//! - the Parts' own hooks: `random_tick`, `can_survive`, `is_valid_bonemeal_target`,
//!   `is_bonemeal_success`, `perform_bonemeal` (the sapling) and `use_on` (the acorn).
//! - `server_tick`: one growth pulse per tree every [`runtime::PULSE_TICKS`], and the record
//!   written when it changed.
//! - `server_stopping` / `world_lifecycle` (close): a last flush of the record, then forget it,
//!   so a session that switches worlds does not carry one save's trees into another.
//! - `register_commands` / `run_mod_command`: `/dynamictrees info <pos>` and `/dynamictrees list`.
//!
//! The numbers and the plan are [`runtime`]'s, which knows nothing about the game; [`glue`] is
//! the transport. What this slice leaves out (every other species, the tree-pack API, the
//! cell-kit solver, the pouch, worldgen) is recorded in `notes.md`.

mod glue;
pub mod runtime;

use mod_api::{
    self, Command, Flow, Hooks, RegisterCommandsEvent, Registrar, RegistrationContext,
    RunModCommandEvent, ServerHost, ServerStoppingEvent, ServerTickEvent, ServerTickPhase,
    WorldLifecycleEvent,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// Declare the two Parts: the sapling block's behaviour and the acorn item's planting.
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        glue::register(registrar);
    }

    /// The end of a running server tick: grow every tree that is due, then flush the record.
    /// A frozen step does not advance the world, so it is ignored.
    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        let running = match event.phase {
            ServerTickPhase::ServerTickEnd { world_running } => world_running,
            // The phase list is not exhaustive, and `ServerTickStart` is not the end of a tick.
            _ => return,
        };
        glue::on_server_tick(host, running);
    }

    /// The server is stopping with its worlds still open: write the record one last time.
    fn server_stopping(&self, host: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        glue::on_server_stopping(host);
    }

    /// A world closed: flush the record and forget it, so the next world starts from its own
    /// file. There is no open phase at this pin, so loading happens at the first running tick.
    fn world_lifecycle(&self, host: &mut dyn ServerHost, event: &mut WorldLifecycleEvent<'_>) {
        glue::on_world_lifecycle(host, event.phase);
    }

    /// `/dynamictrees info <pos>` and `/dynamictrees list`: the readout the harness and players
    /// use. A player's own command: no operator level, nothing it does changes the world.
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event
            .commands
            .push(Command::new("dynamictrees info <pos:block_pos>"));
        event.commands.push(Command::new("dynamictrees list"));
    }

    /// Answer the two usages; anything else named `dynamictrees` gets the usage line.
    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        glue::run_command(host, event)
    }
}
