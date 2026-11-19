//! Pure logic for Dynamic Trees' oak: where a sapling may stand, the plan a tree grows by,
//! its named stages, and the world record that keeps a tree across a relaunch.
//!
//! The numbers are the species' own, read from `trees/dynamictrees/species/oak.json` and the
//! class defaults beside it (`FINDINGS.md`): `signal_energy 12.0`, `growth_rate 0.8`,
//! `tapering 0.3`, and the class default `lowest_branch_height 3`. Upstream turns the energy
//! into geometry with its cell-kit solver; this port replaces the solver with a deterministic
//! seeded plan that spends the same budget (one energy per block walked), which is the one
//! deviation this slice's notes record.
//!
//! Nothing here knows about the game: the glue reads blocks, calls these functions and writes
//! the result back.

/// The oak's `signal_energy`: the growth signal walks this many blocks before it dies.
pub const SIGNAL_ENERGY: u32 = 12;
/// The oak's `growth_rate`: the chance one pulse grows the tree.
pub const GROWTH_RATE: f32 = 0.8;
/// The class default `lowest_branch_height`: no branch in the first three blocks.
pub const LOWEST_BRANCH_HEIGHT: u32 = 3;
/// The oak's `tapering`. Recorded, not modelled: every branch cell is one log block.
pub const TAPERING: f32 = 0.3;
/// How many blocks one branch walks outward (`FINDINGS.md`: the solver's branches are short).
pub const BRANCH_LENGTH: u32 = 2;
/// How many server ticks one growth pulse waits.
pub const PULSE_TICKS: u64 = 20;

/// The acorn item.
pub const OAK_SEED: &str = "dynamictrees:oak_seed";
/// The sapling block the acorn plants.
pub const OAK_SAPLING: &str = "dynamictrees:oak_sapling";
/// The trunk and branch block.
pub const OAK_BRANCH: &str = "dynamictrees:oak_branch";
/// The canopy block.
pub const OAK_LEAVES: &str = "dynamictrees:oak_leaves";
/// The empty cell a sapling is planted into.
pub const AIR: &str = "minecraft:air";

/// The soils whose `acceptable_soils` include `dirt_like` (the oak's standard soils), as the
/// jar's `trees/dynamictrees/soil_properties/*.json` declare them.
pub const DIRT_LIKE: [&str; 9] = [
    "minecraft:dirt",
    "minecraft:grass_block",
    "minecraft:podzol",
    "minecraft:coarse_dirt",
    "minecraft:rooted_dirt",
    "minecraft:mycelium",
    "minecraft:moss_block",
    "minecraft:farmland",
    "minecraft:mud",
];

/// Whether `block` is a soil the oak accepts (upstream `Species.setStandardSoils` →
/// `dirt_like`, matched through the soil properties' `acceptable_soils`).
#[must_use]
pub fn is_acceptable_soil(block: &str) -> bool {
    DIRT_LIKE.contains(&block)
}

/// Whether a sapling may be planted: an acceptable soil below, an empty cell for it and air
/// above it, and no branch or sapling beside it (upstream `DynamicSaplingBlock.canSaplingStay`,
/// which the sapling's own `canSurvive` repeats).
#[must_use]
pub fn can_plant(soil: &str, target: &str, above: &str, neighbours: &[&str]) -> bool {
    target == AIR && can_stay(soil, above, neighbours)
}

/// Whether a sapling already at its cell stays there: upstream's `canSaplingStay` without the
/// planting cell's own check (the sapling is in it).
#[must_use]
pub fn can_stay(soil: &str, above: &str, neighbours: &[&str]) -> bool {
    is_acceptable_soil(soil)
        && above == AIR
        && !neighbours
            .iter()
            .any(|block| *block == OAK_SAPLING || *block == OAK_BRANCH)
}

/// Whether a sapling may grow into its tree: upstream's `shouldTransitionToTree` (air above,
/// an acceptable soil below).
#[must_use]
pub fn can_sprout(soil: &str, above: &str) -> bool {
    is_acceptable_soil(soil) && above == AIR
}

/// The named stages a tree passes through, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// The sapling on the soil: nothing of the tree exists yet.
    Sapling,
    /// Upstream's `transitionToTree`: the trunk base and one leaf above it.
    Sprout,
    /// The trunk is still running up.
    Trunk,
    /// The trunk is done and the branches are growing out.
    Branches,
    /// The branches are done and the canopy is filling in.
    Canopy,
    /// Every step of the plan is placed.
    Mature,
}

impl Stage {
    /// The stage's name, as `/dynamictrees info` and the e2e read it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sapling => "sapling",
            Self::Sprout => "sprout",
            Self::Trunk => "trunk",
            Self::Branches => "branches",
            Self::Canopy => "canopy",
            Self::Mature => "mature",
        }
    }
}

/// What one step places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// Trunk or branch cells: [`OAK_BRANCH`].
    Branch,
    /// Canopy cells: [`OAK_LEAVES`].
    Leaf,
}

/// One growth step: the blocks a single pulse writes. A trunk step is one block so the height
/// visibly rises; a canopy step is one leaf layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// What the step places.
    pub kind: StepKind,
    /// The cells it writes, in order.
    pub blocks: Vec<[i32; 3]>,
}

/// The plan a tree of one seed grows by: its trunk height and its steps, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The sapling position: the trunk's base cell.
    pub base: [i32; 3],
    /// How many blocks the trunk rises above the base (the base counts as one).
    pub trunk_height: u32,
    /// How many branches leave the trunk.
    pub branch_count: u32,
    /// Every step, trunk first, then branches, then the canopy.
    pub steps: Vec<Step>,
}

impl Plan {
    /// The plan for a tree at `base` grown from `seed`. Deterministic: the same pair always
    /// gives the same plan, and different seeds give different trees.
    #[must_use]
    pub fn new(base: [i32; 3], seed: u64) -> Self {
        let branch_count = 2 + u32::from(mix(seed, 1) & 1 == 0);
        // The signal spends one energy per block. The trunk runs until the first branch takes
        // the rest: the branches cost `BRANCH_LENGTH` energy each, and the base cell costs one.
        let trunk_height = SIGNAL_ENERGY
            .saturating_sub(branch_count * BRANCH_LENGTH + 1)
            .max(LOWEST_BRANCH_HEIGHT + 1);
        let heights = branch_heights(trunk_height, branch_count);
        let directions = branch_directions(seed, branch_count);

        let mut steps = Vec::new();
        // Trunk: the base cell first (it replaces the sapling), then up.
        for up in 0..trunk_height {
            steps.push(Step {
                kind: StepKind::Branch,
                blocks: vec![[base[0], base[1] + up as i32, base[2]]],
            });
        }
        // Branches: outward from the trunk at their own height, one cell per step.
        let mut tips = Vec::new();
        for (height, direction) in heights.iter().zip(directions) {
            let y = base[1] + *height as i32;
            for out in 1..=BRANCH_LENGTH {
                let cell = [
                    base[0] + direction[0] * out as i32,
                    y,
                    base[2] + direction[2] * out as i32,
                ];
                steps.push(Step {
                    kind: StepKind::Branch,
                    blocks: vec![cell],
                });
            }
            tips.push([
                base[0] + direction[0] * BRANCH_LENGTH as i32,
                y,
                base[2] + direction[2] * BRANCH_LENGTH as i32,
            ]);
        }
        // Canopy: two rings around and above the trunk top, plus a leaf over each branch tip.
        for layer in [trunk_height - 1, trunk_height] {
            let y = base[1] + layer as i32;
            let mut blocks = Vec::new();
            for dx in -2_i32..=2 {
                for dz in -2_i32..=2 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    if dx.abs() + dz.abs() > 3 {
                        continue;
                    }
                    blocks.push([base[0] + dx, y, base[2] + dz]);
                }
            }
            if layer == trunk_height {
                for tip in &tips {
                    blocks.push([tip[0], tip[1] + 1, tip[2]]);
                }
            }
            steps.push(Step {
                kind: StepKind::Leaf,
                blocks,
            });
        }

        Self {
            base,
            trunk_height,
            branch_count,
            steps,
        }
    }

    /// The stage `steps_done` steps into the plan.
    #[must_use]
    pub fn stage(&self, steps_done: u32) -> Stage {
        let trunk = self.trunk_height;
        let branches = trunk + self.branch_count * BRANCH_LENGTH;
        let total = self.steps.len() as u32;
        match steps_done {
            0 => Stage::Sapling,
            1 => Stage::Sprout,
            n if n <= trunk => Stage::Trunk,
            n if n <= branches => Stage::Branches,
            n if n < total => Stage::Canopy,
            _ => Stage::Mature,
        }
    }

    /// How many steps the whole plan has.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.steps.len() as u32
    }

    /// Whether the plan places nothing (never true for a real plan).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// The trunk heights the branches leave from, lowest first: from `LOWEST_BRANCH_HEIGHT` up to
/// the block under the trunk top.
fn branch_heights(trunk_height: u32, branch_count: u32) -> Vec<u32> {
    let top = trunk_height.saturating_sub(1).max(LOWEST_BRANCH_HEIGHT);
    match branch_count {
        0 => Vec::new(),
        1 => vec![top],
        2 => vec![LOWEST_BRANCH_HEIGHT, top],
        _ => vec![LOWEST_BRANCH_HEIGHT, (LOWEST_BRANCH_HEIGHT + top) / 2, top],
    }
}

/// Distinct cardinal directions, rotated by the seed so two trees of one shape do not line up.
fn branch_directions(seed: u64, count: u32) -> Vec<[i32; 3]> {
    const CARDINALS: [[i32; 3]; 4] = [[1, 0, 0], [0, 0, 1], [-1, 0, 0], [0, 0, -1]];
    let start = (mix(seed, 2) % 4) as usize;
    (0..count as usize)
        .map(|index| CARDINALS[(start + index) % 4])
        .collect()
}

/// A growing tree: the record's fields, and the plan they imply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    /// The level's dimension id, so a relaunch finds the level again.
    pub dimension: String,
    /// The sapling position: the trunk's base cell.
    pub base: [i32; 3],
    /// The seed the plan was drawn from.
    pub seed: u64,
    /// Pulses spent so far (grown or not).
    pub pulses: u32,
    /// Steps placed so far.
    pub steps_done: u32,
}

impl Tree {
    /// A tree planted at `base`: the sapling stage, nothing placed yet.
    #[must_use]
    pub fn planted(dimension: impl Into<String>, base: [i32; 3], seed: u64) -> Self {
        Self {
            dimension: dimension.into(),
            base,
            seed,
            pulses: 0,
            steps_done: 0,
        }
    }

    /// The plan this tree grows by.
    #[must_use]
    pub fn plan(&self) -> Plan {
        Plan::new(self.base, self.seed)
    }

    /// The stage the tree is at.
    #[must_use]
    pub fn stage(&self) -> Stage {
        self.plan().stage(self.steps_done)
    }

    /// Upstream's `transitionToTree`: the first step, the trunk base that replaces the
    /// sapling. `None` once the tree has left the sapling stage.
    pub fn sprout(&mut self) -> Option<Step> {
        if self.steps_done != 0 {
            return None;
        }
        let plan = self.plan();
        let step = plan.steps.first().cloned()?;
        self.steps_done = 1;
        Some(step)
    }

    /// One growth pulse: upstream's `growth_rate` gate, then the next step. `None` when the
    /// gate refused this pulse or the plan is finished.
    pub fn pulse(&mut self) -> Option<Step> {
        let grows = grows(self.seed, self.pulses);
        self.pulses = self.pulses.saturating_add(1);
        if !grows {
            return None;
        }
        let plan = self.plan();
        let step = plan.steps.get(self.steps_done as usize).cloned()?;
        self.steps_done = self.steps_done.saturating_add(1);
        Some(step)
    }

    /// How many blocks the tree has placed so far.
    #[must_use]
    pub fn blocks_placed(&self) -> u32 {
        let plan = self.plan();
        plan.steps
            .iter()
            .take(self.steps_done as usize)
            .map(|step| step.blocks.len() as u32)
            .sum()
    }

    /// The tree as one record row: `dimension\tx\ty\tz\tseed\tpulses\tsteps_done`.
    #[must_use]
    pub fn to_row(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.dimension,
            self.base[0],
            self.base[1],
            self.base[2],
            self.seed,
            self.pulses,
            self.steps_done
        )
    }

    /// A tree read back from a row, `None` for a row that is not one.
    #[must_use]
    pub fn from_row(row: &str) -> Option<Self> {
        let fields: Vec<&str> = row.trim().split('\t').collect();
        let [dimension, x, y, z, seed, pulses, steps] = fields.as_slice() else {
            return None;
        };
        if dimension.is_empty() || !dimension.contains(':') {
            return None;
        }
        Some(Self {
            dimension: (*dimension).to_owned(),
            base: [x.parse().ok()?, y.parse().ok()?, z.parse().ok()?],
            seed: seed.parse().ok()?,
            pulses: pulses.parse().ok()?,
            steps_done: steps.parse().ok()?,
        })
    }
}

/// Whether pulse `pulse` of a tree grown from `seed` grows it: upstream's
/// `growthRate > random.nextFloat()` (oak: 0.8), drawn deterministically so a relaunch
/// repeats the same sequence.
#[must_use]
pub fn grows(seed: u64, pulse: u32) -> bool {
    mix(seed, u64::from(pulse) + 3) % 1000 < (GROWTH_RATE * 1000.0) as u64
}

/// SplitMix64: a small, well-mixed hash. `salt` separates the draws one plan makes.
#[must_use]
pub fn mix(seed: u64, salt: u64) -> u64 {
    let mut z = seed.wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirt_is_soil_and_stone_is_not() {
        assert!(is_acceptable_soil("minecraft:grass_block"));
        assert!(is_acceptable_soil("minecraft:podzol"));
        assert!(is_acceptable_soil("minecraft:mud"));
        assert!(!is_acceptable_soil("minecraft:stone"));
        assert!(!is_acceptable_soil("minecraft:sand"));
        assert!(!is_acceptable_soil("minecraft:air"));
    }

    #[test]
    fn a_sapling_needs_air_above_and_an_empty_cell() {
        assert!(can_plant("minecraft:dirt", AIR, AIR, &[]));
        assert!(!can_plant("minecraft:stone", AIR, AIR, &[]));
        assert!(!can_plant("minecraft:dirt", "minecraft:stone", AIR, &[]));
        assert!(!can_plant("minecraft:dirt", AIR, "minecraft:stone", &[]));
    }

    #[test]
    fn a_sapling_stays_only_on_soil_with_air_above() {
        assert!(can_stay("minecraft:grass_block", AIR, &[]));
        assert!(!can_stay("minecraft:stone", AIR, &[]));
        assert!(!can_stay(
            "minecraft:grass_block",
            "minecraft:oak_leaves",
            &[]
        ));
        assert!(!can_stay("minecraft:grass_block", AIR, &[OAK_SAPLING]));
        // A planted sapling's own cell holds the sapling, not air, so `can_plant` is stricter.
        assert!(can_stay("minecraft:dirt", AIR, &["minecraft:stone"]));
        assert!(!can_plant("minecraft:dirt", OAK_SAPLING, AIR, &[]));
    }

    #[test]
    fn a_sapling_sprouts_only_with_room_above_its_soil() {
        assert!(can_sprout("minecraft:dirt", AIR));
        assert!(!can_sprout("minecraft:dirt", "minecraft:stone"));
        assert!(!can_sprout("minecraft:stone", AIR));
    }

    #[test]
    fn an_adjacent_sapling_or_branch_refuses_the_spot() {
        assert!(!can_plant(
            "minecraft:dirt",
            AIR,
            AIR,
            &["minecraft:stone", OAK_SAPLING]
        ));
        assert!(!can_plant(
            "minecraft:dirt",
            AIR,
            AIR,
            &[OAK_BRANCH, "minecraft:air"]
        ));
        assert!(can_plant(
            "minecraft:dirt",
            AIR,
            AIR,
            &["minecraft:stone", "minecraft:air"]
        ));
    }

    #[test]
    fn the_plan_is_the_same_for_one_seed_and_differs_across_seeds() {
        let base = [10, 64, -3];
        assert_eq!(Plan::new(base, 7), Plan::new(base, 7));
        let plans: Vec<Plan> = (0..8).map(|seed| Plan::new(base, seed)).collect();
        assert!(
            plans.windows(2).any(|pair| pair[0] != pair[1]),
            "eight seeds produced one tree: {plans:?}"
        );
    }

    #[test]
    fn the_trunk_runs_up_from_the_base_and_stops_below_the_energy() {
        let base = [0, 70, 0];
        for seed in 0..32 {
            let plan = Plan::new(base, seed);
            assert!(
                (LOWEST_BRANCH_HEIGHT + 1..=SIGNAL_ENERGY).contains(&plan.trunk_height),
                "trunk height {} is outside the signal energy {SIGNAL_ENERGY}",
                plan.trunk_height
            );
            let trunk: Vec<[i32; 3]> = plan.steps[..plan.trunk_height as usize]
                .iter()
                .flat_map(|step| step.blocks.clone())
                .collect();
            assert_eq!(trunk.len(), plan.trunk_height as usize);
            for (up, cell) in trunk.iter().enumerate() {
                assert_eq!(*cell, [base[0], base[1] + up as i32, base[2]]);
            }
        }
    }

    #[test]
    fn no_branch_starts_below_the_lowest_branch_height() {
        let base = [0, 70, 0];
        for seed in 0..32 {
            let plan = Plan::new(base, seed);
            let branch_cells: Vec<[i32; 3]> = plan.steps[plan.trunk_height as usize..]
                .iter()
                .filter(|step| step.kind == StepKind::Branch)
                .flat_map(|step| step.blocks.clone())
                .collect();
            assert!(
                plan.branch_count >= 2,
                "a tree of seed {seed} has {} branches",
                plan.branch_count
            );
            assert_eq!(
                branch_cells.len(),
                (plan.branch_count * BRANCH_LENGTH) as usize
            );
            for cell in branch_cells {
                let height = (cell[1] - base[1]) as u32;
                assert!(
                    height >= LOWEST_BRANCH_HEIGHT,
                    "a branch leaves at height {height}, below {LOWEST_BRANCH_HEIGHT}"
                );
            }
        }
    }

    #[test]
    fn branches_leave_the_trunk_column() {
        let base = [5, 64, 5];
        let plan = Plan::new(base, 11);
        let branch_cells: Vec<[i32; 3]> = plan.steps[plan.trunk_height as usize..]
            .iter()
            .filter(|step| step.kind == StepKind::Branch)
            .flat_map(|step| step.blocks.clone())
            .collect();
        assert!(!branch_cells.is_empty());
        for cell in branch_cells {
            assert!(
                cell[0] != base[0] || cell[2] != base[2],
                "branch cell {cell:?} is inside the trunk column"
            );
        }
    }

    #[test]
    fn the_canopy_sits_above_the_trunk() {
        let base = [0, 64, 0];
        let plan = Plan::new(base, 3);
        let leaves: Vec<[i32; 3]> = plan
            .steps
            .iter()
            .filter(|step| step.kind == StepKind::Leaf)
            .flat_map(|step| step.blocks.clone())
            .collect();
        assert!(leaves.len() >= 20, "only {} leaves", leaves.len());
        for cell in leaves {
            assert!(
                cell[1] > base[1] + 2,
                "a leaf sits at {cell:?}, inside the trunk"
            );
            assert!((cell[0] - base[0]).abs() <= 2 && (cell[2] - base[2]).abs() <= 2);
        }
    }

    #[test]
    fn the_stages_advance_in_order() {
        let plan = Plan::new([0, 64, 0], 5);
        let stages: Vec<Stage> = (0..=plan.len()).map(|done| plan.stage(done)).collect();
        assert_eq!(stages[0], Stage::Sapling);
        assert_eq!(stages[1], Stage::Sprout);
        assert_eq!(stages[plan.len() as usize], Stage::Mature);
        assert!(
            stages.windows(2).all(|pair| pair[0] <= pair[1]),
            "the stages went backwards: {stages:?}"
        );
        assert_eq!(
            stages
                .iter()
                .filter(|stage| **stage == Stage::Trunk)
                .count() as u32,
            plan.trunk_height - 1
        );
        assert!(stages.contains(&Stage::Branches));
        assert!(stages.contains(&Stage::Canopy));
    }

    #[test]
    fn the_growth_gate_is_seeded_and_about_eighty_percent() {
        let grown = (0..1000).filter(|pulse| grows(42, *pulse)).count();
        assert!(
            (700..=900).contains(&grown),
            "the gate grew {grown} of 1000 pulses, expected about 800"
        );
        assert_eq!(grows(42, 7), grows(42, 7));
        assert_ne!(
            (0..64).map(|p| grows(1, p)).collect::<Vec<_>>(),
            (0..64).map(|p| grows(2, p)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_pulse_places_one_step_and_never_past_the_last() {
        let mut tree = Tree::planted("minecraft:overworld", [0, 64, 0], 9);
        let first = tree.sprout().expect("a sapling sprouts");
        assert_eq!(first.kind, StepKind::Branch);
        assert_eq!(tree.stage(), Stage::Sprout);
        assert!(tree.sprout().is_none(), "a sprouted tree sprouts twice");

        let total = tree.plan().len();
        let mut placed = 1;
        for _ in 0..2000 {
            if tree.pulse().is_some() {
                placed += 1;
            }
        }
        assert_eq!(
            tree.steps_done, total,
            "the tree stopped before its plan ended"
        );
        assert_eq!(placed, total);
        assert_eq!(tree.stage(), Stage::Mature);
        assert!(tree.pulse().is_none(), "a mature tree keeps growing");
        assert_eq!(
            tree.blocks_placed(),
            tree.plan()
                .steps
                .iter()
                .map(|s| s.blocks.len() as u32)
                .sum::<u32>()
        );
    }

    #[test]
    fn a_row_round_trips() {
        let mut tree = Tree::planted("minecraft:the_nether", [-12, 40, 300], 123456789);
        tree.pulses = 17;
        tree.steps_done = 4;
        let row = tree.to_row();
        assert_eq!(Tree::from_row(&row), Some(tree));
        assert_eq!(row.split('\t').count(), 7);
    }

    #[test]
    fn a_bad_row_is_refused() {
        assert_eq!(Tree::from_row(""), None);
        assert_eq!(Tree::from_row("minecraft:overworld\t1\t2\t3\t4\t5"), None);
        assert_eq!(
            Tree::from_row("minecraft:overworld\tx\t2\t3\t4\t5\t6"),
            None
        );
        assert_eq!(Tree::from_row("overworld\t1\t2\t3\t4\t5\t6"), None);
        assert_eq!(Tree::from_row("\t1\t2\t3\t4\t5\t6"), None);
    }
}
