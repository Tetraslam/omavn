Output docs/public/media/cli-agents.mp4
Set Shell bash
Set FontSize 22
Set Width 1400
Set Height 560
Set Padding 30
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms
Set WindowBar Colorful
Hide
Type "source docs/media-src/demo-env.sh && clear"
Enter
Show
Type "omavn --json list | jq -r '.[] | [.id, .engine, .runner] | @tsv'"
Sleep 400ms
Enter
Sleep 2.5s
Type "omavn --json run dies --print | jq '{program, gameid: .env.GAMEID, locale: .env.LANG}'"
Sleep 400ms
Enter
Sleep 3s
Type "omavn run nonexistent </dev/null; echo exit=$?"
Sleep 400ms
Enter
Sleep 3s
