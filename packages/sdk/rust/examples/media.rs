use omochi_ui::{App, AudioPlayer, Image, Result, Video};

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new(());
    let paths: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(paths.len(), 3, "usage: media photo.png audio.wav clip.webm");
    let photo = app.resources.file(&paths[0], None)?;
    let audio = app.resources.file(&paths[1], None)?;
    let video = app.resources.file(&paths[2], None)?;
    app.surface("main", move |ui, _| {
        ui.add(Image::new(photo.clone()))?;
        ui.add(AudioPlayer::new(audio.clone()))?;
        ui.add(Video::new(video.clone()))
    })?;
    app.start().await
}
