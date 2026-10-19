# Environment for the terminal recordings: a throwaway HOME (so the
# library starts empty) with the test downloads in ~/Downloads.
# Run from the repo root:  vhs docs/media-src/install.tape
# Then re-encode for the site (smaller, 1120 px wide):
#   ffmpeg -i in.mp4 -vf scale=1120:-2 -c:v libx264 -crf 28 -an out.mp4
demo="${OMAVN_DEMO_HOME:-$HOME/.cache/omavn-demo/home}"
mkdir -p "$demo/Downloads"
for d in "Dies irae" "Yumina the Ethereal" Himanatsu_final.zip; do
  [ -e "$demo/Downloads/$d" ] || ln -s "$PWD/test-vns/$d" "$demo/Downloads/$d"
done
export HOME="$demo"
unset OMAVN_HOME XDG_DATA_HOME XDG_CONFIG_HOME XDG_CACHE_HOME HYPRLAND_INSTANCE_SIGNATURE
export PS1='\[\e[38;5;211m\]\w\[\e[0m\] ❯ '
cd ~/Downloads
