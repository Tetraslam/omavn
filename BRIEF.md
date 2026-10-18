# omavn: play visual novels on linux!

- the equivalent of proton/wine and wsl but for visual novels
- hyperperformant and hypercompatible
- should just work:tm:
- cli, agent-first, with good and extremely easy docs for humans too (use https://www.fumadocs.dev/docs)
- cli/docs should have an extremely easy way to submit feedback/requests/etc
- modular and well-designed to later become the base layer on which larger projects can be built (examples: launcher, gui, a steam-like client which fetches from torrents, music library which fetches from in-game assets, etc. whatever users want to build on top :D)

note: i added this readme to be the initial starting point so don't edit it yourself, however also realized we should have a pretty readme eventually later so feel free to move it to a different file and use the readme how you would normally write a readme here :)

to start with testing, i have a folder ./test-vns containing a bunch of visual novels which you can use to test compatibility with. you'll likely know the titles of each better than i do, so feel free to make an index yourself (they're my friend's vns which he bought from jast and he wants to switch to linux so this project will be helpful with helping him switch!) he also added many vns which have really weird/janky download package structure and stuff to make sure it's as accurate to the windows experience as possible and so that you get to test as close to the raw download as possible :) fun stuff like dies irae being several dozen packages and other horrifying things lmao but i'm sure it'll be a fun problem to solve

other resources i found which might be helpful but should not be overindexed on because right now playing vns on linux is not as easy as using, say, uv or pnpm and that's the level of polish i want:
- https://www.vnwiki.xyz/linux/
- https://github.com/fumoctl-bits/Visual-Novels-On-Linux
- https://learnjapanese.moe/vn-linux/#__tabbed_1_2
