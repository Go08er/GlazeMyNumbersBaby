# Length converter: a click in the second field. Run as `sw.sh 1000x800 length.sh`.
app GMNB_MODE=length GMNB_SIZE=420x640
shot 0-start
click 26 220      # the second field (Inches)
shot 1-second-field
keys 5
shot 2-typed
