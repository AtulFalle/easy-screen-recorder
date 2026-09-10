use std::path::PathBuf;

fn installer_iss() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("installer/LightCapture.iss");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn installer_script_has_required_directives() {
    let iss = installer_iss();
    for needle in [
        "AppId={{8F3A1C2E-6B47-4D91-9E20-5C7A4B1D8E63}",
        "PrivilegesRequired=lowest",
        "PrivilegesRequiredOverridesAllowed=dialog",
        "AppMutex=LightCapture-tray",
        "SetupMutex=LightCapture-setup",
        "CloseApplications=yes",
        "LicenseFile=..\\LICENSE",
        "DefaultDirName={autopf}\\{#MyAppName}",
        "ArchitecturesAllowed=x64compatible",
        "Name: \"desktopicon\"",
        "nowait postinstall skipifsilent",
        "Source: \"..\\dist\\LightCapture.exe\"",
        "DestDir: \"{app}\"",
        "{autoprograms}\\{#MyAppName}",
        "{autodesktop}\\{#MyAppName}",
        "OutputBaseFilename=LightCapture-Setup-{#MyAppVersion}",
        "MinVersion=10.0.18362",
    ] {
        assert!(
            iss.contains(needle),
            "installer/LightCapture.iss missing {needle:?}"
        );
    }
    let lower = iss.to_ascii_lowercase();
    assert!(
        !lower.contains("hkcu")
            && !lower.contains("runatstartup")
            && !lower.contains("startupfolder"),
        "installer must not register login auto-start"
    );
    assert!(
        !iss.contains("lightcapture-cli"),
        "installer must not ship the CLI"
    );
}
