use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon = out.join("meter.ico");
    // A small vector-like gauge rendered deterministically into the executable icon.
    let n = 32usize;
    let mut dib = vec![];
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(n as i32).to_le_bytes());
    dib.extend_from_slice(&((n * 2) as i32).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&[0; 24]);
    for y in (0..n).rev() {
        for x in 0..n {
            let d = (x as f64 - 15.5).hypot(y as f64 - 15.5);
            let gauge =
                (9.0..=13.5).contains(&d) || ((14..=17).contains(&x) && (10..=19).contains(&y));
            dib.extend_from_slice(if gauge {
                &[0xc8, 0xd8, 0x59, 255]
            } else {
                &[0x25, 0x1e, 0x19, 255]
            });
        }
    }
    dib.extend_from_slice(&vec![0; n * n / 8]);
    let mut ico = vec![0, 0, 1, 0, 1, 0, n as u8, n as u8, 0, 0, 1, 0, 32, 0];
    ico.extend_from_slice(&(dib.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&dib);
    fs::write(&icon, ico).unwrap();
    let manifest = out.join("meter.manifest");
    fs::write(&manifest, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="0.1.0.0" processorArchitecture="amd64" name="CodexMeter" type="win32"/>
  <description>Codex Meter</description>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security></trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application></compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness><longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware></windowsSettings></application>
</assembly>"#).unwrap();
    let resource = out.join("meter.rc");
    fs::write(
        &resource,
        format!(
            "1 ICON \"{}\"\n1 24 \"{}\"\n",
            icon.to_string_lossy().replace('\\', "/"),
            manifest.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap();
    let gnu = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "gnu";
    let result = if gnu {
        let obj = out.join("meter.o");
        let mut command = Command::new("windres.exe");
        if let Ok(linker) = env::var("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER") {
            command.arg(format!("--preprocessor={linker}")).args([
                "--preprocessor-arg=-E",
                "--preprocessor-arg=-xc",
                "--preprocessor-arg=-DRC_INVOKED",
            ]);
        }
        command
            .arg(&resource)
            .args(["-O", "coff", "-o"])
            .arg(&obj)
            .status()
            .map(|s| (s, obj))
    } else {
        let res = out.join("meter.res");
        Command::new("rc.exe")
            .arg("/nologo")
            .arg(format!("/fo{}", res.display()))
            .arg(&resource)
            .status()
            .map(|s| (s, res))
    };
    match result {
        Ok((status, path)) if status.success() => {
            println!("cargo:rustc-link-arg={}", path.display())
        }
        _ => println!(
            "cargo:warning=Resource compiler unavailable: EXE icon and manifest omitted (runtime DPI setup remains enabled)."
        ),
    }
}
