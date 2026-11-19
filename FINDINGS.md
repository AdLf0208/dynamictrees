# FINDINGS — Dynamic Trees 1.7.2, the oak

Source: `ports/_shared/upstream/dynamictrees` (shallow clone of
`DynamicTreesTeam/DynamicTrees`, branch `release/1.21.1`, commit
`8bf66d83ac3a707fe482782fe21d2cb732527c66`), plus the jar at
`var/dynamictrees/dynamictrees.jar` for the shipped data. Read for behaviour; the Rust is a
rewrite, not a translation.

## What grows, and from what

- A seed item (`item/Seed.java`) used on soil plants the species' sapling. The sapling is a
  `DynamicSaplingBlock` (`block/sapling/DynamicSaplingBlock.java`):
  - `randomTick` → if `species.canSaplingGrowNaturally(level, pos)` → `performBonemeal`.
  - `performBonemeal` → if `canSurvive` → `species.transitionToTree(level, pos)`, else drop
    the block.
  - `canSurvive` = `canSaplingStay`: no adjacent branch or sapling in the four horizontal
    directions, air above, and `species.isAcceptableSoil(below)`.
  - `isValidBonemealTarget` / `isBonemealSuccess` = `canBoneMealTree() && canSaplingGrow`.
- `canSaplingGrow` = the soil below is one of `acceptableBlocksForGrowth` (empty for oak, so
  always true); the real soil test is `isAcceptableSoil`, which asks the soil's flags against
  the species' `soilTypeFlags`, and `Species.setStandardSoils` sets that to `dirt_like`.
- `transitionToTree` (`tree/species/Species.java`): the sapling's own position becomes a
  **single branch of the family's primary thickness**, a **leaf block is placed above**, and
  the soil below becomes **rooty dirt at fertility 15** (a `SpeciesBlockEntity` when the
  species needs one).

## How the tree grows afterwards

- The rooty soil block's `randomTick` (`block/soil/SoilBlock.java`) is the driver: a
  `treeGrowthMultiplier` gate, then `updateTree` → `species.update(...)`.
- `Species.update` → `grow`: while `growthRate > random.nextFloat()` (oak: `growth_rate 0.8`)
  it sends a `GrowSignal` from the trunk base; the signal starts at `signal_energy` (oak: 12)
  and `GrowSignal.step()` spends **1 energy per block** (`--energy`; at ≤ 0 the signal fails).
  A pulse that fails still costs a soil-fertility draw.
- Direction choice (`GrowthLogicKit.selectNewDirection`): no branch below
  `lowest_branch_height` (oak: 3, the class default) — it keeps going up; above it, a weighted
  map over the six directions: `up_probability` (2) up, a bonus for the current direction,
  plus what each neighbour is (air 1, leaves 2, branches 2 + radius). So the trunk runs up and
  turns into branches once it is tall enough, and the remaining energy goes sideways.
- Oak has no growth-logic kit of its own: it uses the default kit, so `signal_energy`,
  `up_probability` and `lowest_branch_height` alone shape it. `tapering` (0.3) shrinks the
  radius per step; `max_branch_radius` (8) caps it.
- Leaves are a `DynamicLeavesBlock` that ages by hydro (a cell-kit voxmap), not vanilla's
  distance. They are placed by the branch's own leaf logic as the branch grows.

## What the slice keeps, and what it does not

Kept as numbers: `signal_energy 12` (the trunk can run 12 blocks before the signal dies),
`growth_rate 0.8` (a seeded per-pulse gate), `lowest_branch_height 3` (no branch in the first
three blocks), `tapering 0.3` (recorded, not modelled — every branch block is one log block),
the `dirt_like` soil set, the "air above / no adjacent sapling or branch" placement rule, and
`transitionToTree`'s shape (trunk base + one leaf above at the sprout).

Not modelled: the cell-kit solver that turns energy into the real branch geometry, radius,
rot, disease, fruit, voluntary seed drops, the falling-tree entity, worldgen replacement, and
every other species. The slice's plan is a deterministic seeded model (see `runtime.rs`).
