# Dynamic Trees — port notes

**Class / tier:** `content` / `deep` (Horizon's `docs/modpacks.md`: a content mod that adds
gameplay; deep because it replaces the game's tree growth with trees that grow procedurally
over time and ships a Java API for tree-pack add-ons).

**Licence:** MIT (rank 0). Read from the artefact, not from a listing:
`META-INF/neoforge.mods.toml` in `var/dynamictrees/dynamictrees.jar` (1.7.2, 3,399,913 B)
says `license = "MIT"`, and the repository's `LICENSE` at `release/1.21.1`
(commit `8bf66d83ac3a707fe482782fe21d2cb732527c66`, 1,080 B, sha256
`a04e25a4…38fb0e`) is the plain MIT text with **no asset carve-out**. The jar ships no
`LICENSE` file of its own, so [`LICENSE`](LICENSE) and [`LICENSE-NOTICE.md`](LICENSE-NOTICE.md)
travel inside the package.

**Out of the pack queue — an explicit operator request.** `dynamictrees` is **not in ATM10**
(no ledger row, no source-map row) and **not in the Horizon registry**
(`ports/_shared/horizon-mods/registry-ids.txt` has no `dynamictrees`). The campaign records
the exception rather than hiding it: this port was requested by the operator outside
`python3 ports/tools/port_queue.py`, so it has no queue rank and no pack priority. Nothing
here changes the pack's order.

## The jar inventory (read from `var/dynamictrees/dynamictrees.jar`, not from its name)

1,506 entries (unzip's own count): 725 Java classes (`com/dtteam/dynamictrees/**`, an API a
Horizon port cannot host), 471 JSON files, 53 textures, 29 sounds and one `.nbt`. The block/item
families it declares, from `assets/dynamictrees/blockstates/` and
`assets/dynamictrees/models/item/`:

| Family | Blocks | Items |
|---|---|---|
| oak | `oak_sapling`, `oak_branch`, `stripped_oak_branch`, `oak_leaves` | `oak_seed` ("Oak Acorn") |
| oak (undergrowth variant) | `oak_undergrowth_leaves` | — |
| acacia, birch, cherry, dark oak, jungle, mangrove, spruce, crimson, warped | the same four shapes each (`*_sapling`, `*_branch`, `stripped_*_branch`, `*_leaves`) | `*_seed` |
| azalea, apple oak, cocoa, swamp oak, tall birch, mega crimson/jungle/spruce/warped | saplings (and `apple`) | the matching seeds |
| rooty soils | 36 `rooty_*` blocks (dirt, grass_block, podzol, sand, stone, netherrack, …) | — |
| other | `potted_sapling`, `trunk_shell`, `roots`-shaped roots blocks | `dirt_bucket`, `staff`, `dendro_potion`, `manual` |

What the oak family does, from `trees/dynamictrees/` and the classes the jar ships:

- `trees/dynamictrees/species/oak.json` — `family: oak`, `tapering: 0.3`, `signal_energy: 12.0`,
  `growth_rate: 0.8`, `leaves_properties: oak`, `primitive_sapling: oak_sapling`,
  `preferred_climate: temperate`. Class defaults it relies on: `up_probability: 2`,
  `lowest_branch_height: 3`, acceptable soils `dirt_like`.
- `trees/dynamictrees/families/oak.json` — `common_leaves: oak`, `primitive_log: oak_log`,
  `primitive_stripped_log: stripped_oak_log`, `max_branch_radius: 8`.
- `trees/dynamictrees/leaves_properties/oak.json` — `primitive_leaves: minecraft:oak_leaves`.
- `trees/dynamictrees/soil_properties/*.json` — the `dirt_like` soils (dirt, grass_block,
  podzol, coarse_dirt, rooted_dirt, mycelium, moss_block, farmland, mud).
- `data/dynamictrees/loot_table/trees/{branches,leaves,voluntary}/oak.json` — a felled branch
  drops `minecraft:oak_log` + `minecraft:stick` (through the mod's own
  `multiply_logs_count`/`multiply_sticks_count` functions), leaves drop `oak_seed` at a
  table-bonus chance, and a tree drops a voluntary `oak_seed`.
- `assets/dynamictrees/blockstates/oak_branch.json` → `dynamictrees:block/oak_branch`, a model
  with `"loader": "dynamictrees:branch"` (bark sides, rings top); `oak_sapling.json` →
  `dynamictrees:block/saplings/oak`, a `dynamictrees:smartmodel/sapling`;
  `oak_leaves.json` → `minecraft:block/oak_leaves`; `models/item/oak_seed.json` →
  `dynamictrees:item/standard_seed` + `dynamictrees:item/oak_seed` texture.

## Slice 1 — what this package does

One species, the oak, as a flat SDK `.hmod` (`api = "0.2"`, game 26.3, pin
`0b14f8acfd97fa2c750e42795249dcc981178ca4` + mixins `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`):

- **`dynamictrees:oak_seed`** (the Oak Acorn) is used on valid soil and plants
  **`dynamictrees:oak_sapling`** above it, refusing an invalid spot (upstream's
  `DynamicSaplingBlock.canSaplingStay`: air above, a `dirt_like` soil below, no adjacent
  branch or sapling). The seed is not consumed on a refused spot.
- **Growth starts** on the sapling's own random tick or on a bone-meal click
  (`is_valid_bonemeal_target` / `is_bonemeal_success` / `perform_bonemeal`), which is
  upstream's `DynamicSaplingBlock.randomTick` → `performBonemeal` → `transitionToTree`.
- **Growth proceeds over time** through named stages — `sapling`, `sprout`, `trunk`,
  `branches`, `canopy`, `mature` — one block per pulse, a pulse every 20 server ticks, with
  upstream's `growth_rate 0.8` as a seeded per-pulse gate. The trunk is `dynamictrees:oak_branch`
  blocks, the branches are `oak_branch` too, and the canopy is `dynamictrees:oak_leaves`.
- **The tree persists**: the growing trees live in a world-level record,
  `<save>/horizon-mod-data/dynamictrees/trees.tsv`, written through
  `host.world_data_files()`, so a relaunch resumes the tree exactly where it stopped.

### Why a world-level record and not a block entity

The pin reverts a block entity's own writes inside its tick (a measured platform fact,
`ARCHITECTURE.md` §3), and a growing tree spans many positions — trunk, branches, leaves — so
no single block entity owns it. A record keyed by the tree's base position is one small file,
survives chunk unload, and is written by the tick that changes it. A block entity per tree is
the upstream-like alternative; it is not needed for one species.

### Gaps (deliberately out of slice 1, all recorded)

- Every other species (acacia, birch, cherry, dark oak, jungle, mangrove, spruce, crimson,
  warped, azalea, apple oak, cocoa, swamp oak, tall birch, the four mega species) and their
  seeds.
- The **tree-pack API** (`com/dtteam/dynamictrees/api/**`, `treepack/`) — Java interfaces and
  registry hooks for add-on packs; a Horizon port cannot host another mod's Java classes.
- The **cell-kit solver** (`systems/cell`, `worldgen/JoCode`, `GrowthLogicKits`): the real
  branch shape is solved per species with a voxel cell kit. This slice's plan is a
  deterministic seeded model that honours the species' numbers (signal energy, tapering,
  growth rate, lowest branch height) but not the solver.
- The **dendro pouch**, the **Woodland Staff**, the **dendro potions**, the **dirt bucket**,
  the **manual**, the **config screens** (`api/configuration`), the **Jade/WTHIT** plugins.
- **Worldgen**: `worldgen/configured_feature/dynamic_tree.json` and the NeoForge biome
  modifier that replaces vanilla trees — the pack's trees are unchanged in this slice.
- **Rooty soil**: upstream converts the soil under a sapling to `dynamictrees:rooty_dirt`
  (with fertility and a species block entity) and that block is the growth driver. This slice
  keeps the soil as it is and drives growth from the record; `rooty_*` blocks are not carried.
- **Branch radius/thickness, stripping, rot, disease, seed drops from leaves, fruit, the
  falling-tree entity and its damage type, sounds and particles.**
- **Leaf decay by age** (upstream's dynamic leaves age by hydro); the port's leaves are
  ordinary `minecraft:oak_leaves`-template leaves, kept alive by the trunk being in
  `#minecraft:logs`.

### Deviations from upstream, and why

| Upstream | Here | Why |
|---|---|---|
| sapling is `DynamicSaplingBlock` (a plant with its own random tick) | `dynamictrees:oak_sapling` declared on the `minecraft:poppy` template with `random_ticks: true` and a block Part | the `minecraft:oak_sapling` template's `Class::Sapling` random-tick would grow a vanilla tree over ours; a flower template is a plant on soil with no growth and no bone meal of its own |
| branch blocks render through `dynamictrees:branch` (a radius-aware model loader) | `oak_branch` declared on `minecraft:oak_planks`, drawn with `minecraft:block/oak_log` | the custom model loader is not portable; a log model is the closest vanilla look |
| the sapling model is `dynamictrees:smartmodel/sapling` | `minecraft:block/cross` with `minecraft:block/oak_sapling` | same reason |
| growth is driven by the rooty soil's random tick | driven by `server_tick`, one step every 20 ticks; the *start* is still the sapling's random tick or bone meal | steady, observable growth and an exact relaunch; the record replaces the soil block |
| leaves drop `oak_seed` through `dynamictrees:seed_item` and `dynamictrees:seasonal_seed_drop_chance` | leaves drop nothing; the tree's own voluntary drop is not modelled | the loot entry type and condition are the mod's own code |
| branch loot uses `dynamictrees:multiply_logs_count` | `oak_branch` drops 1 `minecraft:oak_log` (a branch block's own loot table) | the function is the mod's own code; one log per block is the shape it produces |

## Skills

Skills: superpowers/using-superpowers,brainstorming,writing-plans,test-driven-development,systematic-debugging,verification-before-completion; BeautifulRust/rust-best-practices

## Verification (2026-11-19, pin `0b14f8a` / mixins `e59da90`, launcher 0.2.78)

| Check | Result |
|---|---|
| `cargo test --offline` | **15 passed**, 0 failed (`ports/_shared/evidence/dynamictrees/sdk-test.log`) |
| `cargo clippy --all-targets --offline -- -D warnings` | exit 0, **0 warnings** |
| `cargo fmt --check` | clean |
| `cargo horizon-mod build` twice | exit 0 both, **`cmp`-identical** |
| `cargo horizon-mod check` | `dynamictrees 0.1.0 (native-mod, format native-1/0b14f8a/e59da90, for 26.3)`, exit 0 |
| `cargo horizon-mod fits --build 0b14f8a` | `0b14f8a  made for`, exit 0 |
| `tools/carry.py --check` | `PASS: carry check, 27 files, 0 failure(s)` |
| `verify-sdk-package.py dynamictrees` | **1/1** |
| in-game leg | **`PASS` + `EXIT:0`** (`ports/_shared/evidence/dynamictrees-e2e/e2e-launcher.log`) |
| control (same Nightly, no package) | **`EXIT:1`** at `the acorn is not in this game` (`e2e-control.log`) |
| selftest (`SELFTEST=1`) | **`EXIT:1`** at `the first sample has 1 blocks, not a whole tree` (`e2e-selftest.log`) |
| `check-e2e-logs.py dynamictrees-e2e --strict` | `logs=3 ok=3`, exit 0 |

Delivered: [`dynamictrees.hmod`](dynamictrees.hmod) **32,536 B**, SHA-256
`777d0d4bd09559e6f049fae33596d0433a19f4c2a372893293aa61b0c51c5987`, 35 entries.
The transcript that proves growth over time (six samples, six distinct rising counts, stages
`sprout → trunk → branches → canopy → mature`, the record surviving a relaunch and resuming)
is quoted in [`VALIDATION.md`](VALIDATION.md).

Two things learned in game and recorded there: a content block's `sounds` property takes a
**block** id (a sound-type id refuses the whole pack at load), and a cross-shaped plant is
clicked by block position rather than by its top face.

A world close (`world_lifecycle`'s `WorldCloseFinal` / `WorldCloseFailed`) flushes the record
and forgets it, so a session that switches worlds reads the next world's own file at its first
running tick instead of carrying the previous world's trees over.

The readout's `stage=sapling` for a sapling that has not sprouted is a fallback: a fresh
sapling has no record yet (the record starts at `transitionToTree`), so `/dynamictrees info`
reads the block and reports the plan it would grow. That is how the e2e can assert the
sapling's own stage before any bone meal.
