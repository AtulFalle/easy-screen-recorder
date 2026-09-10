use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use windows::core::{IInspectable, Interface, HSTRING};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
};

/// Unpackaged Win32 toasts can use the PowerShell AUMID that ships with Windows.
const POWERSHELL_AUMID: &str = "Microsoft.Windows.PowerShell_8wekyb3d8bbwe!App";
const ARG_OPEN: &str = "open";
const ARG_SHOW: &str = "show";

/// Keep recent toasts alive so in-process `Activated` still fires.
static LIVE: Mutex<Vec<ToastNotification>> = Mutex::new(Vec::new());

pub fn recording_saved(path: &Path) {
    show("Saved recording", path);
}

pub fn recording_kept(title: &str, path: &Path) {
    show(title, path);
}

pub(crate) fn open_file(path: &Path) {
    if let Err(err) = Command::new("explorer").arg(path).spawn() {
        eprintln!("could not open {}: {err}", path.display());
    }
}

pub(crate) fn show_in_folder(path: &Path) {
    let arg = format!("/select,{}", path.display());
    if let Err(err) = Command::new("explorer").arg(arg).spawn() {
        eprintln!("could not show {}: {err}", path.display());
    }
}

fn show(headline: &str, path: &Path) {
    if let Err(err) = show_inner(headline, path) {
        eprintln!("could not show toast: {err}");
    }
}

fn show_inner(headline: &str, path: &Path) -> windows::core::Result<()> {
    let xml = toast_xml(headline, &path.display().to_string());
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let file = path.to_path_buf();
    toast.Activated(&TypedEventHandler::new(
        move |_, args: windows::core::Ref<'_, IInspectable>| {
            let argument = activation_argument(args.as_ref());
            dispatch(&argument, &file);
            Ok(())
        },
    ))?;
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(POWERSHELL_AUMID))?;
    notifier.Show(&toast)?;
    retain(toast);
    Ok(())
}

fn activation_argument(args: Option<&IInspectable>) -> String {
    args.and_then(|args| args.cast::<ToastActivatedEventArgs>().ok())
        .and_then(|args| args.Arguments().ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn dispatch(argument: &str, path: &Path) {
    match parse_toast_action(argument) {
        ToastAction::Show => show_in_folder(path),
        ToastAction::Open => open_file(path),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastAction {
    Open,
    Show,
}

fn parse_toast_action(argument: &str) -> ToastAction {
    if argument == ARG_SHOW {
        ToastAction::Show
    } else {
        ToastAction::Open
    }
}

fn toast_xml(headline: &str, file: &str) -> String {
    format!(
        concat!(
            r#"<toast launch="{launch}">"#,
            r#"<visual><binding template="ToastGeneric">"#,
            r#"<text>LightCapture</text><text>{headline}</text><text>{file}</text>"#,
            r#"</binding></visual>"#,
            r#"<actions>"#,
            r#"<action content="Open" arguments="{launch}"/>"#,
            r#"<action content="Show in folder" arguments="{show}"/>"#,
            r#"</actions></toast>"#,
        ),
        headline = xml_escape(headline),
        file = xml_escape(file),
        launch = ARG_OPEN,
        show = ARG_SHOW,
    )
}

fn retain(toast: ToastNotification) {
    let Ok(mut live) = LIVE.lock() else {
        return;
    };
    live.push(toast);
    if live.len() > 4 {
        live.remove(0);
    }
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml_specials() {
        assert_eq!(xml_escape(r#"a&b<c>"#), "a&amp;b&lt;c&gt;");
    }

    #[test]
    fn toast_xml_has_open_and_show_actions() {
        let xml = toast_xml("Saved recording", r"C:\Videos\clip.mp4");
        assert!(xml.contains(r#"arguments="open""#));
        assert!(xml.contains(r#"arguments="show""#));
        assert!(xml.contains("Open"));
        assert!(xml.contains("Show in folder"));
        assert!(xml.contains("Saved recording"));
        assert!(xml.contains(r"C:\Videos\clip.mp4"));
    }

    #[test]
    fn body_click_and_open_open_the_file() {
        assert_eq!(parse_toast_action(""), ToastAction::Open);
        assert_eq!(parse_toast_action("open"), ToastAction::Open);
        assert_eq!(parse_toast_action("show"), ToastAction::Show);
    }
}
