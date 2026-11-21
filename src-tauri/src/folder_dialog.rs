//! Native folder selection on the application's UI thread, never under timer locks.
use std::path::PathBuf;
use tauri::Manager;

pub fn pick_folder(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    previous: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    // GTK's empty Recent view cannot select a directory. Start in a real folder.
    let previous = previous
        .filter(|folder| folder.is_dir())
        .or_else(|| app.path().home_dir().ok());
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let window = window.clone();
        // GTK is already initialized by Tauri. Do not start a second GTK thread/loop.
        app.run_on_main_thread(move || {
            let result = (|| {
                let parent = window.gtk_window().map_err(|e| e.to_string())?;
                let dialog = gtk::FileChooserNative::new(
                    Some("Выберите папку хранилища для выгрузки"),
                    Some(&parent),
                    gtk::FileChooserAction::SelectFolder,
                    Some("Выбрать"),
                    Some("Отмена"),
                );
                dialog.set_modal(true);
                if let Some(folder) = previous {
                    dialog.set_current_folder(folder);
                }
                let response = dialog.run();
                let folder = if response == gtk::ResponseType::Accept {
                    dialog.filename()
                } else {
                    None
                };
                dialog.destroy();
                Ok(folder)
            })();
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
        receiver.recv().map_err(|e| e.to_string())?
    }
    #[cfg(windows)]
    {
        use tauri_plugin_dialog::DialogExt;
        let mut dialog = app
            .dialog()
            .file()
            .set_title("Выберите папку хранилища для выгрузки")
            .set_parent(window);
        if let Some(folder) = previous {
            dialog = dialog.set_directory(folder);
        }
        dialog
            .blocking_pick_folder()
            .map(|folder| folder.into_path().map_err(|e| e.to_string()))
            .transpose()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (app, window, previous);
        Err("Выбор папки поддерживается на Linux и Windows".into())
    }
}
