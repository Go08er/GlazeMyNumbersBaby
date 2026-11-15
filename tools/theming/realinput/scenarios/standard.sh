# Standard page, as a person: click the 7 key, type +2=, Ctrl+H opens the
# History sheet in a narrow window, Escape closes it. Sourced by sw.sh; run it
# as `sw.sh 1000x800 standard.sh` (the coordinates are for a 420x640 window at
# the screen's top left). For DGMNB: APP=dgmnb with the DGMNB_* env.
if [ "${APP:-gmnb}" = dgmnb ]; then
  app DGMNB_MODE=standard DGMNB_SIZE=420x640
else
  app GMNB_MODE=standard GMNB_SIZE=420x640
fi
shot 0-start
click 62 382       # the 7 key
shot 1-seven
keys '+2='
shot 2-sum
keys -M ctrl -k h -m ctrl
wait_ 1
shot 3-history
keys -k Escape
wait_ 1
shot 4-closed
