Test audio for the stream player (DEC-048): 440 Hz tones, made with ffmpeg:

    ffmpeg -f lavfi -i "sine=frequency=440:duration=4:sample_rate=44100" -ac 1 -b:a 48k tone.mp3
    ... -c:a aac -b:a 48k tone.m4a; ... -f adts tone.aac
    ... sample_rate=22050 ... -c:a libvorbis -q:a 0 tone.ogg; ... -c:a flac tone.flac
    ffmpeg -f lavfi -i "sine=frequency=440:duration=30:sample_rate=22050" -ac 1 -b:a 32k long.mp3
    ffmpeg -f lavfi -i "sine=frequency=330:duration=150:sample_rate=8000" -ac 1 -b:a 8k episode.mp3
