# Dynamic Trees, slice 1 — implementation plan

**Goal:** one species (oak) grows from a sapling into a dynamic tree over time — trunk,
branches and leaves placed progressively and persisted — with a fail-capable in-game leg.

**Shape:** one flat SDK package (`api = "0.2"`), content (`assets/`, `data/`) + one
`impl Hooks` (`src/lib.rs`) + pure logic (`src/runtime.rs`).

**Spec:** `ports/_shared/queue/brief-dynamictrees.md`; behaviour extracted from
Dynamic Trees 1.7.2 (`release/1.21.1`, commit `8bf66d83…`) — `FINDINGS.md`.

## Global constraints

- Pin: Horizon `0b14f8acfd97fa2c750e42795249dcc981178ca4`, mixins
  `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, game 26.3.
- Licence MIT (rank 0), read from the jar's `mods.toml` and the repo's `LICENSE`.
- No bare `cargo`: every build/test through `ports/tools/build-slot.sh run`.
  `CARGO_INCREMENTAL=0`; `rm -f Cargo.lock` before each package build.
- `runtime.rs` holds every number and rule, with no `mod_api` type; glue only reads,
  calls it and writes back.

## Design decisions (and why)

1. **Growth state lives in a world-level record**, not a block entity: the pin reverts a
   block entity's own writes inside its tick (measured platform fact), and a tree spans
   many positions, so no single block entity owns it. One file,
   `<save>/horizon-mod-data/dynamictrees/trees.tsv`, through `host.world_data_files()`.
2. **The growth process is driven by `server_tick`** (one step every `PULSE_TICKS`), so
   growth is steady and observable; the *start* is upstream's own: the sapling's random
   tick or a bone-meal click. This also makes a relaunch resume exactly.
3. **Sapling template is `minecraft:poppy`**, not `minecraft:oak_sapling`: at 26.3 the
   sapling template's `Class::Sapling` random-tick replaces the block with a vanilla tree.
   A flower template is a plant on soil with no growth and no bone meal of its own.
4. **Branch blocks are declared on `minecraft:oak_planks`** (a stateless solid block) and
   drawn with `minecraft:block/oak_log`; upstream's radius model loader is not portable.
   They join `#minecraft:logs` so vanilla leaves keep their distance.

## Tasks

### Task 1 — skeleton and inventory
- `horizon-mod.toml`, `Cargo.toml`, `.gitignore`, `LICENSE` (upstream MIT),
  `LICENSE-NOTICE.md`, `notes.md` (jar inventory, class/tier, slice, gaps, exception).
- Test: none (documents only). Verify: `cargo horizon-mod check` later.

### Task 2 — placement rules in `runtime.rs` (test first)
- `is_acceptable_soil(&str) -> bool` — upstream's `dirt_like` set.
- `can_plant(soil, above, neighbours) -> bool` — upstream `DynamicSaplingBlock.canSaplingStay`.
- `can_survive(...)` — the same rule minus the "air above" requirement for updates.

### Task 3 — the growth plan (test first)
- `Plan::new(base, seed)` from `SIGNAL_ENERGY 12`, `GROWTH_RATE 0.8`,
  `LOWEST_BRANCH_HEIGHT 3`, `TAPERING 0.3`: trunk height, branch segments, leaf cluster.
- `Stage` from `steps_done`: `Sapling`, `Sprout`, `Trunk`, `Branches`, `Canopy`, `Mature`.
- `pulse(seed, pulses) -> bool` — the seeded 0.8 gate, deterministic per (seed, pulse).
- `Tree::advance(...) -> Option<Step>` — the next block to place.

### Task 4 — persistence (test first)
- `Tree::to_row()` / `Tree::from_row()` — one TSV row per tree, tolerant parse.

### Task 5 — glue (`src/lib.rs`)
- Block part on `dynamictrees:oak_sapling`: `random_tick`, `can_survive`,
  `is_valid_bonemeal_target`, `is_bonemeal_success`, `perform_bonemeal`.
- Item part on `dynamictrees:oak_seed`: `use_on` plants the sapling on valid soil.
- `server_tick`: one step per tree every 20 ticks; flush the record when it changed.
- `register_commands`/`run_mod_command`: `/dynamictrees info <pos>` and
  `/dynamictrees list`.

### Task 6 — content carry
- `data/dynamictrees/horizon/block/{oak_sapling,oak_branch,oak_leaves}.json`,
  `data/dynamictrees/horizon/item/oak_seed.json`, blockstates, models, the carried
  `oak_seed.png`, loot tables (vanilla-shaped), tags (branch/leaf/sapling/seed + the
  `minecraft:logs` join), lang, and `tools/carry.py --check`.

### Task 7 — ladder
`rm -f Cargo.lock`; `cargo test --offline`; `cargo clippy --all-targets --offline --
-D warnings`; `cargo fmt --check`; two `cargo horizon-mod build` runs `cmp`-identical;
`cargo horizon-mod check`; `cargo horizon-mod fits --build 0b14f8a`;
`verify-sdk-package.py dynamictrees` 1/1.

### Task 8 — in-game leg
`launcher-game.sh build dynamictrees` then `run`; `tools/e2e.sh` plants a seed on
grass, samples the growth every 20 ticks (stage order, strictly rising block counts),
asserts the sapling is gone, relaunches and asserts the tree survived and resumed;
control (`GAME=$CONTROL_GAME`) fails at the first assertion; `check-e2e-logs
dynamictrees-e2e --strict` green.
