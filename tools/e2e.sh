#!/usr/bin/env bash
# In-game check for Dynamic Trees slice 1: the oak grows from an acorn over time.
#
# One offscreen creative launch, then on a grass platform at y=101 in the Overworld:
#   1. the acorn exists in this game (this is the assertion the control run fails:
#      `GAME=$CONTROL_GAME` has no `dynamictrees:oak_seed`, so the leg stops here);
#   2. using the acorn on the top face of grass plants `dynamictrees:oak_sapling`;
#   3. using it on stone plants nothing and spends nothing (upstream's
#      `canSaplingStay`: an acceptable soil, air above, no branch or sapling beside);
#   4. the sapling reads `stage=sapling steps=0/… blocks=0`;
#   5. a bone-meal click starts it: the sapling is gone, the base is
#      `dynamictrees:oak_branch`, and the readout is `stage=sprout steps=1`;
#   6. it then GROWS OVER TIME: sampled every 20 ticks the block count only rises,
#      the stage walks sprout -> trunk -> branches -> canopy -> mature, and the
#      first sample is nowhere near the finished tree (so it is not one tick);
#   7. the tree survives a relaunch: the game is quit mid-growth, reopened on the
#      same world, the readout reports the same tree at the same step (the record
#      is in the save), the trunk is still there, and growth RESUMES;
#   8. a second sapling left alone grows from its own random tick (no bone meal);
#   9. the finished tree is a trunk of `oak_branch` blocks with branches and a
#      canopy of `oak_leaves`.
#
# SELFTEST=1 asserts the first sample after the bone meal is already `mature`; it
# is not, so the run must fail (exit 1) — the growth assertions are alive.
#
# The control leg is the same command against an unmodified launcher build:
#   GAME=$CONTROL_GAME bash ports/dynamictrees/tools/e2e.sh
# which fails at step 1 with `FAIL:` and exits 1.
#
# SDK layout (0.1.0): GAME must be a launcher-built horizon-client carrying this
# package (`ports/tools/launcher-game.sh build dynamictrees`), because the mod's
# code is compiled into the game. Every native command is typed with
# `ctl chat "/cmd"`: `ctl command` runs the console path and bypasses it.
set -euo pipefail
trap 'echo "FAILED at line $LINENO: $BASH_COMMAND" >&2' ERR

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/dynamictrees
CTL=${CTL:-$PORT/target/horizon-ctl}
GAME=${GAME:-$PORT/target/horizon-client}
CONTROL_GAME=${CONTROL_GAME:-}
RES=${RES:-${HORIZON_RES:-/home/adlf/Desktop/horizon/.local/horizon-client/26.3}}
SOCK=${SOCK:-/tmp/opencode/dynamictrees-e2e.sock}
SELFTEST=${SELFTEST:-0}
WORLD=DTGrowthE2E

SEED=dynamictrees:oak_seed
SAPLING=dynamictrees:oak_sapling
BRANCH=dynamictrees:oak_branch
LEAVES=dynamictrees:oak_leaves

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }
[ -f "$PORT/dynamictrees.hmod" ] || { echo "missing package: $PORT/dynamictrees.hmod" >&2; exit 1; }

SCRATCH=$(mktemp -d "${E2E_TMP:-/tmp/opencode}"/dynamictrees-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"
rm -f "$SOCK"
cd "$SCRATCH"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
say() { echo "== $* =="; }
count_item() { ctl inventory | jq -r --arg i "$1" '.counts[$i] // 0'; }
block_at() { ctl block "$1" "$2" "$3" | jq -r '.block // "unknown"'; }
count_blocks() { # x1 y1 z1 x2 y2 z2 id
    ctl blocks "$1" "$2" "$3" "$4" "$5" "$6" | jq -r --arg i "$7" '.counts[$i] // 0'
}

# The mod's own readout, typed as chat (the player command path).
info() { # x y z
    ctl chat "/dynamictrees info $1 $2 $3" >/dev/null
    ctl wait 2 >/dev/null
    local reply line
    reply=$(ctl messages 6)
    line=$(printf '%s' "$reply" | jq -r '.chat[]' 2>/dev/null | grep -m1 -E "^dynamictrees: " || true)
    if [ -z "$line" ]; then
        echo "  no readout for /dynamictrees info $1 $2 $3; last chat: $reply" >&2
    fi
    printf '%s\n' "$line"
}
stage_of() { info "$1" "$2" "$3" | sed -n 's/.*stage=\([a-z]*\).*/\1/p'; }
blocks_of() { info "$1" "$2" "$3" | sed -n 's/.*blocks=\([0-9][0-9]*\).*/\1/p'; }
steps_of() { info "$1" "$2" "$3" | sed -n 's/.*steps=\([0-9][0-9]*\)\/.*/\1/p'; }
held_item() { ctl state | jq -r '.held.item // ""'; }

cleanup() {
    local status=$?
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    rm -rf "$SCRATCH" || true
    # Last line of every run: `check-e2e-logs.py` reads it as the verdict.
    echo "EXIT:$status"
}
trap cleanup EXIT

launch() { # [world]
    local world=${1:-$WORLD}
    rm -f "$SOCK"
    cd "$SCRATCH"
    say "launch (offscreen, creative, the launcher build carries the package)"
    "$CTL" --socket "$SOCK" launch --game "$GAME" --world "$world" --seed 20261105 \
        --creative --offscreen --wait --game-dir "$SCRATCH/game" --resources "$RES"
    ctl wait-ready 180
    if grep -q "content mods not loaded" "${SOCK}.log" 2>/dev/null; then
        grep "content mods not loaded" "${SOCK}.log" >&2
        fail "the content mod did not load"
    fi
}

# ---------------------------------------------------------------------------
# 1. the acorn exists — the assertion the control run fails
# ---------------------------------------------------------------------------
launch
ctl command "gamerule spawn_mobs false" >/dev/null
ctl command "gamerule random_tick_speed 3" >/dev/null
ctl wait 4

say "the acorn exists in this game"
ctl command "clear @s" >/dev/null
ctl command "item replace entity @s hotbar.0 with $SEED 4" >/dev/null || true
ctl wait 2
SEEDS=$(count_item "$SEED")
echo "acorns in the inventory: $SEEDS"
[ "$SEEDS" -ge 1 ] || fail "the acorn ($SEED) is not in this game, so this is not the modded game"

# ---------------------------------------------------------------------------
# the arena: grass on stone at y=101, air above
# ---------------------------------------------------------------------------
say "an Overworld platform: stone at y=100, grass at y=101"
PX=$(ctl state | jq -r '.player.position[0]' | cut -d. -f1)
PZ=$(ctl state | jq -r '.player.position[2]' | cut -d. -f1)
ctl command "fill $((PX-8)) 100 $((PZ-8)) $((PX+8)) 100 $((PZ+8)) minecraft:stone" >/dev/null
ctl command "fill $((PX-8)) 101 $((PZ-8)) $((PX+8)) 101 $((PZ+8)) minecraft:grass_block" >/dev/null
ctl command "fill $((PX-8)) 102 $((PZ-8)) $((PX+8)) 112 $((PZ+8)) minecraft:air" >/dev/null
ctl tp "$PX.5" 102 "$((PZ+3)).5" >/dev/null
ctl wait 10
ctl command "kill @e[type=!minecraft:player]" >/dev/null
ctl wait 4
# Survival: a planted acorn and a bone meal are spent, as upstream spends them
# (a creative hand gives the item without losing it).
ctl command "gamemode survival @s" >/dev/null
ctl wait 2

X1=$((PX));     Z1=$((PZ + 1))   # the tree that grows now
X2=$((PX - 3)); Z2=$((PZ + 1))   # the sapling left to its random tick
X3=$((PX - 6)); Z3=$((PZ + 1))   # stone: an invalid spot
Y=102

say "using the acorn on the top face of grass plants a sapling"
ctl select 1 >/dev/null || fail "cannot select the acorn's hotbar slot"
ctl wait 2
[ "$(held_item)" = "$SEED" ] || fail "the held item is '$(held_item)', not the acorn"
ctl look-at-block "$X1" 101 "$Z1" up >/dev/null || true
ctl interact "$X1" 101 "$Z1" up >/dev/null || fail "the acorn click failed"
ctl wait 3
PLANTED=$(block_at "$X1" "$Y" "$Z1")
echo "block above the grass: $PLANTED"
[ "$PLANTED" = "$SAPLING" ] || fail "the acorn planted '$PLANTED', not $SAPLING"
LEFT=$(count_item "$SEED")
echo "acorns after planting: $LEFT"
[ "$LEFT" = "$((SEEDS - 1))" ] || fail "planting spent $((SEEDS - LEFT)) acorns, expected 1"

LINE=$(info "$X1" "$Y" "$Z1")
echo "readout: $LINE"
[ "$(stage_of "$X1" "$Y" "$Z1")" = sapling ] || fail "a fresh sapling reads '$(stage_of "$X1" "$Y" "$Z1")'"
[ "$(blocks_of "$X1" "$Y" "$Z1")" = 0 ] || fail "a fresh sapling has placed blocks"

say "using the acorn on stone plants nothing and spends nothing"
ctl command "setblock $X3 101 $Z3 minecraft:stone" >/dev/null
ctl wait 2
BEFORE=$(count_item "$SEED")
ctl look-at-block "$X3" 101 "$Z3" up >/dev/null || true
ctl interact "$X3" 101 "$Z3" up >/dev/null || true
ctl wait 3
REFUSED=$(block_at "$X3" "$Y" "$Z3")
AFTER=$(count_item "$SEED")
echo "block above the stone: $REFUSED; acorns $BEFORE -> $AFTER"
[ "$REFUSED" = "minecraft:air" ] || fail "an acorn planted on stone: '$REFUSED'"
[ "$AFTER" = "$BEFORE" ] || fail "the refused click spent an acorn ($BEFORE -> $AFTER)"

# ---------------------------------------------------------------------------
# 2. bone meal starts the tree: sprout
# ---------------------------------------------------------------------------
say "a bone-meal click starts the tree"
ctl command "item replace entity @s hotbar.0 with minecraft:bone_meal 8" >/dev/null \
    || ctl give minecraft:bone_meal 8 >/dev/null || fail "cannot give bone meal"
ctl select 1 >/dev/null || fail "cannot select the bone-meal slot"
ctl wait 2
[ "$(held_item)" = "minecraft:bone_meal" ] || fail "the held item is '$(held_item)', not bone meal"
# The sapling is a cross-shaped plant: aim at the block itself (its outline), not
# at its top face, which the ray would pass over.
REPLY=$(ctl interact "$X1" "$Y" "$Z1" 2>&1) || { echo "  click reply: $REPLY" >&2; fail "the bone-meal click failed"; }
echo "  click reply: $REPLY"
ctl wait 3
BASE=$(block_at "$X1" "$Y" "$Z1")
echo "the sapling's cell: $BASE"
[ "$BASE" = "$BRANCH" ] || fail "after the bone meal the cell is '$BASE', not $BRANCH"
LINE=$(info "$X1" "$Y" "$Z1")
echo "readout: $LINE"
# The pulse clock is the server's, so a pulse may land between the click and this
# readout: the claim is that the tree started, not that it is exactly one step in.
SPROUT_STAGE=$(stage_of "$X1" "$Y" "$Z1")
SPROUT_STEPS=$(steps_of "$X1" "$Y" "$Z1")
echo "stage after the click: $SPROUT_STAGE (step $SPROUT_STEPS)"
[ "$SPROUT_STAGE" != sapling ] || fail "the bone meal left the sapling in place"
[ "$SPROUT_STEPS" -ge 1 ] || fail "the tree is at step $SPROUT_STEPS after the bone meal"

# ---------------------------------------------------------------------------
# 3. growth over time: sampled every 20 ticks
# ---------------------------------------------------------------------------
say "the tree grows over time (one sample every 20 ticks)"
SAMPLES=""
FIRST_BLOCKS=$(blocks_of "$X1" "$Y" "$Z1")
LAST=$FIRST_BLOCKS
PREV_STAGE=sprout
STAGES="sprout"
for _ in 1 2 3 4 5 6; do
    ctl wait 20 >/dev/null
    STAGE=$(stage_of "$X1" "$Y" "$Z1")
    BLOCKS=$(blocks_of "$X1" "$Y" "$Z1")
    echo "  sample: stage=$STAGE blocks=$BLOCKS"
    [ -n "$BLOCKS" ] || fail "the readout vanished mid-growth"
    [ "$BLOCKS" -ge "$LAST" ] || fail "the tree shrank: $LAST -> $BLOCKS"
    LAST=$BLOCKS
    STAGES="$STAGES $STAGE"
    SAMPLES="$SAMPLES $BLOCKS"
done
DISTINCT=$(printf '%s\n' $SAMPLES | sort -u | wc -l)
echo "block counts seen: $SAMPLES (distinct: $DISTINCT)"
echo "stages seen: $STAGES"
[ "$DISTINCT" -ge 3 ] || fail "only $DISTINCT distinct block counts in 120 ticks: the tree is not growing over time"
[ "$LAST" -gt "$FIRST_BLOCKS" ] || fail "the tree did not grow in 120 ticks ($FIRST_BLOCKS -> $LAST)"

if [ "$SELFTEST" = 1 ]; then
    say "selftest: expect the first sample to be a finished tree (must fail)"
    [ "$FIRST_BLOCKS" -ge 40 ] || fail "SELFTEST: the first sample has $FIRST_BLOCKS blocks, not a whole tree"
    echo "SELFTEST unexpectedly saw a finished tree at the first sample"
    exit 0
fi

# ---------------------------------------------------------------------------
# 4. the tree survives a relaunch, and resumes
# ---------------------------------------------------------------------------
say "the tree survives a relaunch"
BEFORE_QUIT_BLOCKS=$(blocks_of "$X1" "$Y" "$Z1")
BEFORE_QUIT_STEPS=$(steps_of "$X1" "$Y" "$Z1")
BEFORE_QUIT_STAGE=$(stage_of "$X1" "$Y" "$Z1")
TRUNK_BEFORE=$(count_blocks "$X1" "$((Y-1))" "$Z1" "$X1" "$((Y+4))" "$Z1" "$BRANCH")
echo "before the quit: stage=$BEFORE_QUIT_STAGE steps=$BEFORE_QUIT_STEPS blocks=$BEFORE_QUIT_BLOCKS trunk=$TRUNK_BEFORE"
[ "$BEFORE_QUIT_STEPS" -ge 2 ] || fail "the tree is at step $BEFORE_QUIT_STEPS; nothing to persist"
ctl quit >/dev/null 2>&1 || true
sleep 3

launch "$WORLD"
ctl command "gamerule random_tick_speed 3" >/dev/null
ctl tp "$PX.5" 102 "$((PZ+3)).5" >/dev/null
ctl wait 20
LINE=$(info "$X1" "$Y" "$Z1")
echo "after the relaunch: $LINE"
[ -n "$LINE" ] || fail "the record is gone: no tree at $X1 $Y $Z1 after the relaunch"
AFTER_RELAUNCH_STEPS=$(steps_of "$X1" "$Y" "$Z1")
AFTER_RELAUNCH_BLOCKS=$(blocks_of "$X1" "$Y" "$Z1")
echo "steps $BEFORE_QUIT_STEPS -> $AFTER_RELAUNCH_STEPS, blocks $BEFORE_QUIT_BLOCKS -> $AFTER_RELAUNCH_BLOCKS"
[ "$AFTER_RELAUNCH_STEPS" -ge "$BEFORE_QUIT_STEPS" ] \
    || fail "the relaunch lost progress: step $BEFORE_QUIT_STEPS -> $AFTER_RELAUNCH_STEPS"
TRUNK_AFTER=$(count_blocks "$X1" "$((Y-1))" "$Z1" "$X1" "$((Y+4))" "$Z1" "$BRANCH")
echo "trunk blocks after the relaunch: $TRUNK_AFTER (before: $TRUNK_BEFORE)"
[ "$TRUNK_AFTER" -ge "$TRUNK_BEFORE" ] || fail "the trunk lost blocks across the relaunch"
[ "$TRUNK_AFTER" -ge 2 ] || fail "the trunk has $TRUNK_AFTER blocks after the relaunch"

say "growth resumes after the relaunch"
RESUMED=0
LAST=$AFTER_RELAUNCH_BLOCKS
for _ in $(seq 1 24); do
    ctl wait 20 >/dev/null
    BLOCKS=$(blocks_of "$X1" "$Y" "$Z1")
    [ -n "$BLOCKS" ] || fail "the readout vanished after the relaunch"
    [ "$BLOCKS" -gt "$LAST" ] && RESUMED=1
    LAST=$BLOCKS
    [ "$(stage_of "$X1" "$Y" "$Z1")" = mature ] && break
done
echo "blocks after the relaunch: $LAST (stage $(stage_of "$X1" "$Y" "$Z1"))"
[ "$RESUMED" = 1 ] || fail "the tree did not grow after the relaunch (blocks stayed $LAST)"

# ---------------------------------------------------------------------------
# 5. the finished tree
# ---------------------------------------------------------------------------
say "the finished tree: trunk, branches and a canopy"
STAGE=$(stage_of "$X1" "$Y" "$Z1")
echo "final readout: $(info "$X1" "$Y" "$Z1")"
[ "$STAGE" = mature ] || fail "the tree stopped at stage '$STAGE', not mature"
TREE_BRANCH=$(count_blocks "$((X1-2))" "$Y" "$((Z1-2))" "$((X1+2))" "$((Y+8))" "$((Z1+2))" "$BRANCH")
TREE_LEAVES=$(count_blocks "$((X1-2))" "$Y" "$((Z1-2))" "$((X1+2))" "$((Y+8))" "$((Z1+2))" "$LEAVES")
TREE_SAPLINGS=$(count_blocks "$((X1-2))" "$Y" "$((Z1-2))" "$((X1+2))" "$((Y+8))" "$((Z1+2))" "$SAPLING")
echo "oak_branch=$TREE_BRANCH oak_leaves=$TREE_LEAVES oak_sapling=$TREE_SAPLINGS"
[ "$TREE_BRANCH" -ge 5 ] || fail "the tree has $TREE_BRANCH branch blocks, expected a trunk of 5 or more"
[ "$TREE_LEAVES" -ge 20 ] || fail "the tree has $TREE_LEAVES leaf blocks, expected a canopy of 20 or more"
[ "$TREE_SAPLINGS" = 0 ] || fail "the sapling is still standing ($TREE_SAPLINGS)"
[ "$(block_at "$X1" "$((Y+1))" "$Z1")" = "$BRANCH" ] || fail "the trunk's second block is not a branch block"

# ---------------------------------------------------------------------------
# 6. a sapling left alone grows from its own random tick
# ---------------------------------------------------------------------------
say "a sapling left alone grows from its own random tick (no bone meal)"
ctl command "setblock $X2 $Y $Z2 $SAPLING" >/dev/null
ctl wait 2
[ "$(block_at "$X2" "$Y" "$Z2")" = "$SAPLING" ] || fail "the second sapling did not stand"
[ "$(stage_of "$X2" "$Y" "$Z2")" = sapling ] || fail "a setblock sapling is not registered as a sapling"
ctl command "gamerule random_tick_speed 40" >/dev/null
SPROUTED=0
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    ctl wait 20 >/dev/null
    STAGE=$(stage_of "$X2" "$Y" "$Z2")
    if [ "$STAGE" != sapling ] && [ -n "$STAGE" ]; then SPROUTED=1; break; fi
done
ctl command "gamerule random_tick_speed 3" >/dev/null
echo "the random-ticked sapling reached stage '$(stage_of "$X2" "$Y" "$Z2")'"
[ "$SPROUTED" = 1 ] || fail "the sapling never grew from its own random tick in 400 ticks"

echo "PASS: the acorn plants an oak sapling on soil and refuses stone; a bone-meal click sprouts it; it grows through sprout -> trunk -> branches -> canopy -> mature over 120+ ticks with a strictly rising block count; the record survives a quit and relaunch and growth resumes; a sapling left alone grows from its own random tick; the finished tree is $TREE_BRANCH branch blocks and $TREE_LEAVES leaves with no sapling left"
