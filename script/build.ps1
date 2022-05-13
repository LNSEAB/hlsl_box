$root = $PSScriptRoot
$dxc_bin = "$root/../target/dxc/bin/x64"

$dxc_ps1 = "$root/dxc.ps1"
$dxc_args = "-Command $dxc_ps1"
Start-Process "pwsh.exe" -ArgumentList $dxc_args -WorkingDirectory $root -NoNewWindow -Wait

if ($args.Contains("release")) {
    $target_dir = "$root/../target/release"
    cargo.exe build --release
} else {
    $target_dir = "$root/../target/debug"
    cargo.exe build
    Copy-Item "$dxc_bin/dxcompiler.dll" "$target_dir/deps"
    Copy-Item "$dxc_bin/dxil.dll" "$target_dir/deps"
}
Copy-Item "$dxc_bin/dxcompiler.dll" $target_dir
Copy-Item "$dxc_bin/dxil.dll" $target_dir
