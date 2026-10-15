# My free games list

`free-games.json` is GameShelf's own list of free homebrew games. GameShelf reads it from
this repository (the `main` branch), so a change here shows up in the app the next time
the Free games tab opens. No new build needed.

The game files aren't stored here: each one downloads straight from its maker's GitHub
release.

## Add a game

Copy an entry and change it:

```json
{
  "id": "short-unique-name",
  "title": "Name shown in GameShelf",
  "system": "Genesis",
  "developer": "who made it",
  "description": "One line about the game.",
  "tags": ["platformer"],
  "website": "https://github.com/owner/repo",
  "cover": "https://opengraph.githubassets.com/1/owner/repo",
  "files": ["https://github.com/owner/repo/releases/download/v1.0/game.bin"]
}
```

- **system**: `Ps1`, `Ps2`, `Ps3`, `Psp`, `GameCube`, `Nes`, `Snes`, `N64`, `Gb`, `Gbc`,
  `Gba`, `Nds`, `Genesis` or `Dreamcast`.
- **files**: the release download link(s). Several files (like a `.cue` and its `.bin`)
  go into one folder named after the game. A `.zip` is unpacked.
- **cover** (optional): any image link. `opengraph.githubassets.com/1/<owner>/<repo>`
  gives the repository's preview card.

To find the link of a release file: open the repository's **Releases**, right-click the
file and copy its link.
