import { App } from '@omochi-ui/sdk';

const app = new App({});
const [photo, audio, video] = process.argv.slice(2, 5).map(file => app.resources.file(file));
if (!photo || !audio || !video) throw new Error('usage: app.ts photo.png audio.wav clip.webm');
app.surface('main', ui => {
  ui.image(photo, { description: 'Registered image' });
  ui.audioPlayer(audio, { description: 'Registered audio' });
  ui.video(video);
});
await app.start();
