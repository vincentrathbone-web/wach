//! Downloads from WhatsApp: ask where to save (Windows' own Save-as window),
//! then a "Saved" toast whose click opens the file.
//!
//! WebView2's `DownloadStarting` event is taken with a deferral, so the
//! Save-as window opens after the event has returned (never inside
//! WebView2's own callback).

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

use tauri::AppHandle;
use webview2_com::{Microsoft::Web::WebView2::Win32::*, StateChangedEventHandler, take_pwstr};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
        UI::{
            Shell::{
                Common::{COMDLG_FILTERSPEC, ITEMIDLIST},
                FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FileSaveDialog, IFileSaveDialog, ILCreateFromPathW, ILFree, IShellItem,
                SHCreateItemFromParsingName, SHOpenFolderAndSelectItems, SIGDN_FILESYSPATH, ShellExecuteW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

use crate::toast;

struct Pending {
    args: ICoreWebView2DownloadStartingEventArgs,
    deferral: ICoreWebView2Deferral,
    owner: isize,
}

thread_local! {
    /// Downloads waiting for the Save-as window. COM objects bound to the UI
    /// thread, so they live here (like the notifications in hooks.rs).
    static PENDING: RefCell<(u64, HashMap<u64, Pending>)> = RefCell::new((0, HashMap::new()));
}

/// Called from WebView2's `DownloadStarting` (UI thread).
pub fn on_starting(app: &AppHandle, owner: HWND, args: ICoreWebView2DownloadStartingEventArgs) -> windows::core::Result<()> {
    unsafe {
        // We show our own window and toast instead of Edge's download bubble.
        args.SetHandled(true)?;
        let deferral = args.GetDeferral()?;
        let key = PENDING.with(|p| {
            let mut p = p.borrow_mut();
            p.0 += 1;
            let key = p.0;
            p.1.insert(key, Pending { args, deferral, owner: owner.0 as isize });
            key
        });
        // Hop through another thread so the dialog runs after this event
        // has returned, back on the UI thread.
        let app2 = app.clone();
        std::thread::spawn(move || {
            let app3 = app2.clone();
            let _ = app2.run_on_main_thread(move || ask(&app3, key));
        });
    }
    Ok(())
}

fn ask(app: &AppHandle, key: u64) {
    let Some(p) = PENDING.with(|p| p.borrow_mut().1.remove(&key)) else { return };
    unsafe {
        let suggested = PathBuf::from(read(|s| p.args.ResultFilePath(s)));
        match save_dialog(HWND(p.owner as _), &suggested) {
            Some(path) => {
                let _ = p.args.SetResultFilePath(&HSTRING::from(path.as_os_str()));
                if let Ok(op) = p.args.DownloadOperation() {
                    watch(app, &op);
                }
            }
            None => {
                let _ = p.args.SetCancel(true);
            }
        }
        let _ = p.deferral.Complete();
    }
}

/// Toast when the download has finished (or failed).
unsafe fn watch(app: &AppHandle, op: &ICoreWebView2DownloadOperation) {
    let mut token = 0i64;
    let app = app.clone();
    let handler = StateChangedEventHandler::create(Box::new(move |sender, _| unsafe {
        let Some(op) = sender else { return Ok(()) };
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        op.State(&mut state)?;
        let path = PathBuf::from(read(|s| op.ResultFilePath(s)));
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
            let folder = path.parent().map(|d| d.display().to_string()).unwrap_or_default();
            let (target, app2) = (path.clone(), app.clone());
            toast::show_saved(&format!("Saved {name}"), &folder, move |arg| {
                // Shell calls want the UI thread (COM is set up there).
                let _ = app2.run_on_main_thread(move || {
                    if arg == "folder" { show_in_folder(&target) } else { open_file(&target) }
                });
            });
        } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
            let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
            op.InterruptReason(&mut reason)?;
            if reason != COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED {
                toast::show("download", &format!("Couldn't save {name}"), "The download stopped before it finished.", "", || {});
            }
        }
        Ok(())
    }));
    unsafe {
        let _ = op.add_StateChanged(&handler, &mut token);
    }
}

/// Windows' Save-as window. `None` when the user cancels.
unsafe fn save_dialog(owner: HWND, suggested: &Path) -> Option<PathBuf> {
    unsafe {
        let dlg: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        if let Some(name) = suggested.file_name() {
            let _ = dlg.SetFileName(&HSTRING::from(name));
        }
        // Keep the file's own type first so Windows doesn't change it.
        let ext = suggested.extension().map(|e| e.to_string_lossy().into_owned()).filter(|e| !e.is_empty());
        if let Some(ext) = &ext {
            let label = HSTRING::from(format!("{} file (*.{ext})", ext.to_uppercase()));
            let pattern = HSTRING::from(format!("*.{ext}"));
            let specs = [
                COMDLG_FILTERSPEC { pszName: PCWSTR(label.as_ptr()), pszSpec: PCWSTR(pattern.as_ptr()) },
                COMDLG_FILTERSPEC { pszName: w!("All files (*.*)"), pszSpec: w!("*.*") },
            ];
            let _ = dlg.SetFileTypes(&specs);
            let _ = dlg.SetDefaultExtension(&HSTRING::from(ext.as_str()));
        }
        // Downloads the first time; after that Windows remembers the last folder.
        if let Some(dir) = suggested.parent()
            && let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(dir), None)
        {
            let _ = dlg.SetDefaultFolder(&item);
        }
        if let Ok(opts) = dlg.GetOptions() {
            let _ = dlg.SetOptions(opts | FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM);
        }
        dlg.Show(Some(owner)).ok()?; // an error here means "Cancel"
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        path.map(PathBuf::from)
    }
}

fn open_file(path: &Path) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Explorer with the file selected.
fn show_in_folder(path: &Path) {
    unsafe {
        let pidl: *mut ITEMIDLIST = ILCreateFromPathW(&HSTRING::from(path));
        if pidl.is_null() {
            return;
        }
        let _ = SHOpenFolderAndSelectItems(pidl, None, 0);
        ILFree(Some(pidl));
    }
}

unsafe fn read(f: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut p = PWSTR::null();
    match f(&mut p) {
        Ok(()) if !p.is_null() => take_pwstr(p),
        _ => String::new(),
    }
}
