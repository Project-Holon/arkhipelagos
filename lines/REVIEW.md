# In-game lines: format and review

## Format

- `en.json` holds the source lines. Each line has an id, a kind
  (`item`, `item-name`, `game` or `menu`), a source (a BWAP message it
  replaces, or `arkh` for a new line) and the text.
- `fr.json`, `de.json`, `it.json` and `es.json` map each id to its text.
  `ja.json` gives two texts per id: `kana` and `kanji`, one for each of the
  game's two Japanese text blocks.
- Official names are never typed in. A placeholder takes them from the
  cartridge's own text: `{item:N}`, `{species:N}`, `{place:N}`, `{name:N}`
  (people and seasons) and `{badge:N}`. The `terms` table in `en.json`
  names the text file for each kind.
- Other tokens: `\n` new line, `\c` clear the box, `\r` scroll one line,
  `{c:N}` text colour, `{buf:N}` a buffer the script fills.
- A language file may list a line under `own_terms`, with a reason, when
  its grammar needs a form of a name that the cartridge does not hold.
  The checker then only asks that `{c:N}` and `{buf:N}` match English.

## Checks

`python tools/checklines.py` checks every line in every language on every
cartridge of that language (12 cartridges):

- every placeholder resolves from that cartridge's text;
- a translation holds the same placeholders as English;
- every character is in the game's font;
- no line is wider than the game's own widest line of the same kind
  (a `{buf:N}` counts as wide as the widest item name);
- at most two lines per message box, item descriptions at most the game's
  line count, menu entries one line;
- the Japanese kana block holds no kanji (except 円, as in the game).

`python tools/linesheet.py` writes `out/lines-sheet.txt`: every line in
every language with the Black cartridge's names filled in, for reading.

## Second review pass

Read from the rendered sheet, line by line against English. Names and
articles were checked against the game's own text with
`tools/grepgame.py`.

Fixed:

- Articles before place names, as the game writes them:
  FR "du Château Enfoui", "des Vestiges du Rêve", "dans le Mont Foré",
  "d'Ogoesse"; IT "del Castello Sepolto", "del Cantiere dei Sogni",
  "nel Monte Vite"; ES "del Castillo Ancestral", "del Solar de los
  Sueños", "en el Monte Tuerca"; DE "der Traumbrache", "in den
  Wendelberg".
- DE "im Alten Palast": the cartridge name is "Alter Palast", and German
  needs the inflected form here. Listed under `own_terms`.
- Item genders: DE "ein Silberflügel" (the game: "ein Silberflügel"),
  ES "un Ala Plateada" (the game: "Un Ala Plateada"), FR "un Glas
  Tempête" (glas is masculine), FR "une Peau Métal".
- Seasons: the French names hold their article ("Le printemps", "L'été"),
  which gave "le Le printemps". The line now starts with the name. The
  Italian and Spanish names are in capitals (PRIMAVERA), so those lines
  write the plain word with its article (`own_terms`). English now reads
  "Spring will be here soon."
- English prices use a thousands comma, as the game does ("$1,000").
- FR "Quelle est cette étrange lumière rouge?" (the colour was missing);
  FR "Tu ne peux pas encore passer" ("yet" was missing).

Checked and kept:

- Punctuation follows each cartridge: no space before "!" and "?" in
  French; prices as "$1000" (IT), "1000 $" (ES), "1 000 $" (FR, DE),
  "1000円" (JA).
- Names in capitals in Spanish and Italian (YAKÓN, RAFAN, MEDALLA JET)
  are the cartridge's own style and stay.
- Shortened item names (IT "Camp. Onda", ES "Camp. Oleaje", ES
  "Crán. Dragón", ES "Rev.metálico") are the cartridge's own names.

## Least sure

These read correctly to me, but a native speaker should look at them:

- IT `hiker.offer`: "un Metalcoperta". The game text gives no article
  for this item.
- FR `tidal_bell.no` and `tidal_bell.yes`: "un/le Glas Tempête". The game
  text gives no article for this item; masculine follows "le glas".
- All Japanese lines: the tone (casual for most people, polite for guards
  and the shop) and the kana spacing follow the game's style, but they
  are my own wording.
- The menu words for the seasons (`seasons.choice.*`) are short so that
  they fit one menu line; FR, IT and ES use plain adjectives (Doux,
  Chaud, Frais, Froid).
- The far item name in each language (`item.other.name`): FR "Colis
  lointain", DE "Fernes Item", IT "Invio lontano", ES "Envío lejano",
  JA "とどけもの". These are new words, not game terms.
