"""Write data/world/species_names.json (the 649 species names, English, for
the names of dexsanity and seensanity locations) from PokemonBWAP's
data/pokemon/pokedex.py (MIT). Run once; the file is ours from then on.

    python tools/bwap2names.py <PokemonBWAP apworld folder>"""

import ast
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    src = Path(sys.argv[1]) / "data" / "pokemon" / "pokedex.py"
    tree = ast.parse(src.read_text(encoding="utf-8"))
    names: dict[int, str] = {}
    for node in tree.body:
        if isinstance(node, ast.Assign) and getattr(node.targets[0], "id", "") == "by_number":
            for k, v in zip(node.value.keys, node.value.values):
                names[k.value] = v.value
    out = [""] + [names[i] for i in range(1, 650)]
    data = {"_source": "PokemonBWAP data/pokemon/pokedex.py (MIT), by National Dex number; index 0 unused",
            "names": out}
    (ROOT / "data" / "world" / "species_names.json").write_text(
        json.dumps(data, ensure_ascii=False, indent=0) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(out) - 1} names")


if __name__ == "__main__":
    main()
