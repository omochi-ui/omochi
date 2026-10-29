"""Run with: omochi-ui dev -- python3 examples/media/app.py photo.png audio.wav clip.webm"""
import asyncio
import sys
from omochi_ui import App

app = App({})
photo, audio, video = [app.resources.file(path) for path in sys.argv[1:4]]

@app.surface("main")
def main(ui, state):
    ui.image(photo, description="Registered image")
    ui.audio_player(audio, description="Registered audio")
    ui.video(video)

asyncio.run(app.start())
