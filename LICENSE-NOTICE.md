# Licence notice — Dynamic Trees slice 1

- **Upstream:** Dynamic Trees 1.7.2 for Minecraft 1.21.1 / NeoForge
  (`DynamicTreesTeam/DynamicTrees`, <https://github.com/DynamicTreesTeam/DynamicTrees>),
  branch `release/1.21.1`, commit `8bf66d83ac3a707fe482782fe21d2cb732527c66`.
- **Licence as read:** `license = "MIT"` in the pack jar's `META-INF/neoforge.mods.toml`
  (`var/dynamictrees/dynamictrees.jar`, 1.7.2, 3,399,913 B). The jar ships no `LICENSE` file
  of its own; the repository's `LICENSE` at that commit is the plain MIT text
  (1,080 B, sha256 `a04e25a413917077c6dd99d90a574181ab340c8a0e4d993ba45df2a96538fb0e`) with **no
  asset carve-out** — unlike `the_bumblezone`, whose `LICENSE_ASSETS.txt` is all-rights-reserved.
  That text is reproduced in [`LICENSE`](LICENSE) and travels inside `dynamictrees.hmod`.
- **Carried material:** the oak family's block and item declarations, blockstates, models, the
  `oak_seed` texture and its item model, the English names, the loot tables and the block/item
  tags — all read from the jar (inventory in [`notes.md`](notes.md)).
- **Rust:** a clean-room rewrite against the pin's documented mod API; the upstream Java was
  read for behaviour only (`FINDINGS.md`), never translated.
- **Rank:** 0 (permissive). Publishable under MIT with this source, and only with the
  maintainer's approval (ARCHITECTURE.md §8).
- **Package:** 0.1.0, the flat SDK format (mod API 0.2). This notice and the MIT text travel
  inside `dynamictrees.hmod`, which is the MIT licence's condition on redistribution.
