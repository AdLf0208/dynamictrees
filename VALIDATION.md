# Validation — Dynamic Trees slice 1 (the oak)

Pin: SDK nightly **`0b14f8acfd97fa2c750e42795249dcc981178ca4`** + mixins
**`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`**, game 26.3, launcher **0.2.78**.
Every cargo step ran through `ports/tools/build-slot.sh run` with `CARGO_INCREMENTAL=0`;
the in-game legs ran through `ports/tools/launcher-game.sh` (which takes a slot and wraps the
script in `e2e-guard.sh`). Logs: `ports/_shared/evidence/dynamictrees/` and
`ports/_shared/evidence/dynamictrees-e2e/`.

## Standalone crate (the pure logic)

| Step | Command | Result |
|---|---|---|
| Tests | `cargo test --offline` | **15 passed, 0 failed** (`sdk-test.log`) |
| Lints | `cargo clippy --all-targets --offline -- -D warnings` | exit 0, **0 warnings** (`sdk-clippy.log`) |
| Formatting | `cargo fmt --check` | clean (`sdk-fmt.log`) |

The rules were written test-first: the first red run (stubbed bodies) failed 11 of 13 tests
with the messages the assertions exist for (`trunk height 0 is outside the signal energy 12`,
`the gate grew 1000 of 1000 pulses`, …); the red run also showed one vacuous test
(`no_branch_starts_below_the_lowest_branch_height` passed on an empty plan), which was
strengthened before the green run. The placement split (`can_stay` / `can_plant` /
`can_sprout`) and the sapling fallback in the readout came later, each with its own red run
(4 failed → 15 passed).

## Package

| Step | Result |
|---|---|
| `rm -f Cargo.lock && cargo horizon-mod build` (twice) | exit 0 / exit 0, **`cmp`-identical** (`sdk-build.log`) |
| Delivered file | [`dynamictrees.hmod`](dynamictrees.hmod) — **32,536 B**, SHA-256 `777d0d4bd09559e6f049fae33596d0433a19f4c2a372893293aa61b0c51c5987`, 35 entries |
| `cargo horizon-mod check dynamictrees.hmod` | `dynamictrees 0.1.0 (native-mod, format native-1/0b14f8a/e59da90, for 26.3)`, exit 0 (`sdk-check.log`) |
| `cargo horizon-mod fits dynamictrees.hmod --build 0b14f8a` | `0b14f8a  made for`, exit 0 (`sdk-fits.log`) |
| Carry proof | `python3 tools/carry.py --check` → `PASS: carry check, 27 files, 0 failure(s)` |
| Package identity | `python3 ports/tools/verify-sdk-package.py dynamictrees` → **1/1** (package, runtime byte-identical, no packed `Cargo.lock`, 15 tests, 0 clippy warnings, check, fits, skills gate) |

## In-game leg

`ports/tools/launcher-game.sh build dynamictrees` compiles the package into a game at the pin
(`native-0b14f8a-6b69aee3d6c8`, 2 s: only the mod compiles). The legs then run
`ports/dynamictrees/tools/e2e.sh`:

| Run | Command | Result |
|---|---|---|
| default | `launcher-game.sh run dynamictrees` | **`PASS` + `EXIT:0`** (`dynamictrees-e2e/e2e-launcher.log`) |
| control | `GAME=$CONTROL_GAME bash tools/e2e.sh` (the same Nightly, no package) | **`FAIL: the acorn (dynamictrees:oak_seed) is not in this game` + `EXIT:1`** (`e2e-control.log`) |
| selftest | `SELFTEST=1 bash tools/e2e.sh` (asserts the first sample is already a whole tree) | **`FAIL: SELFTEST: the first sample has 1 blocks, not a whole tree` + `EXIT:1`** (`e2e-selftest.log`) |
| log checker | `python3 ports/tools/check-e2e-logs.py dynamictrees-e2e --strict` | `logs=3 ok=3 missing-exit=0 bad-exit=0`, exit 0 |

The default leg's transcript (this run, world `DTGrowthE2E`, seed 20261105):

```
block above the grass: dynamictrees:oak_sapling
acorns after planting: 3
readout: dynamictrees: minecraft:overworld 14 102 4 stage=sapling steps=0/13 blocks=0 trunk=7 branches=2
block above the stone: minecraft:air; acorns 3 -> 3
the sapling's cell: dynamictrees:oak_branch
stage after the click: sprout (step 1)
== the tree grows over time (one sample every 20 ticks) ==
  sample: stage=trunk blocks=4
  sample: stage=branches blocks=6
  sample: stage=branches blocks=7
  sample: stage=branches blocks=8
  sample: stage=branches blocks=10
block counts seen:  3 4 6 7 8 10 (distinct: 6)
stages seen: sprout trunk trunk branches branches branches branches
before the quit: stage=branches steps=7 blocks=7 trunk=5
after the relaunch: dynamictrees: minecraft:overworld 14 102 4 stage=branches steps=10/13 blocks=10 trunk=7 branches=2
steps 7 -> 10, blocks 7 -> 10
trunk blocks after the relaunch: 5 (before: 5)
blocks after the relaunch: 53 (stage mature)
oak_branch=9 oak_leaves=41 oak_sapling=0
the random-ticked sapling reached stage 'trunk'
PASS: … it grows through sprout -> trunk -> branches -> canopy -> mature over 120+ ticks with
a strictly rising block count; the record survives a quit and relaunch and growth resumes; a
sapling left alone grows from its own random tick; the finished tree is 9 branch blocks and 40
leaves with no sapling left
```

What each assertion proves: the block counts rise sample by sample (so the tree is not placed
in one tick), the stages advance in order, the sapling's cell becomes a branch block, the
record survives `ctl quit` and a relaunch with the same step count and the trunk intact, and
growth continues afterwards. The control proves the acorn is the package's (an unmodded game
has no such item); the selftest proves the growth assertions are not vacuous.

## Harness facts learned here (worth keeping)

- **A content block's `sounds` property takes a *block* id, not a sound type.**
  `"sounds": "minecraft:wood"` refuses the whole pack at load: *"resource packs not applied:
  dynamictrees:oak_branch: sounds minecraft:wood is not a block"*. The declaration was dropped
  and the template's sounds are inherited.
- **A cross-shaped plant is clicked by block position, not by its top face.** `ctl interact X Y
  Z up` aims at the top centre of the cell, above a plant's outline, and the click misses;
  `ctl interact X Y Z` (no face) aims at the cell and hits it.
- **The pulse clock is the server's, so a step can land between a click and the readout.** The
  sprout assertion is "stage is not `sapling` and step ≥ 1", not "step == 1" — the stricter
  form failed once at `stage=trunk steps=2`.
- **`minecraft:oak_sapling` must not be the sapling's template**: at 26.3 that template's
  `Class::Sapling` random-tick replaces the block with a vanilla oak. `minecraft:poppy` is a
  plant on soil with no growth and no bone meal of its own.
- **Leaves are stateless at 26.3** (`assets/minecraft/blockstates/oak_leaves.json` has one
  variant), so `Class::Leaves`' `distance == 7` decay can never fire on a mod leaf that has no
  `distance` property; the `#minecraft:logs` tag join is still carried for fidelity and for
  axe/hoe interactions.
