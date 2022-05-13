$root = $PSScriptRoot

$target_dir = "$root/../target"
$package_dir = "$target_dir/package"
$dir = "$package_dir/hlsl_box"
$workspace_dir = "$root/.."
$dxc_bin = "$target_dir/dxc/bin/x64"

Write-Host "[package.ps1] start"

$dxc_ps1 = "$root/dxc.ps1"
$dxc_args = "-Command $dxc_ps1"
Start-Process "pwsh.exe" -ArgumentList $dxc_args -WorkingDirectory $root -NoNewWindow -Wait

if (Test-Path -Path $dir) {
    Remove-Item -Recurse -Force -Path $package_dir | Out-Null
    Write-Host "[package.ps1] remove package"
}
New-Item $dir -ItemType Directory | Out-Null
Write-Host "[package.ps1] create $dir"

cargo.exe build --profile production
cargo.exe about generate about.hbs > "$dir/license.html"
Copy-Item "$dxc_bin/dxcompiler.dll" $dir
Copy-Item "$dxc_bin/dxil.dll" $dir
Copy-Item "$target_dir/production/hlsl_box.exe" $dir
Copy-Item "$workspace_dir/README.md" $dir
Copy-Item -Recurse -Force "$workspace_dir/include" $dir 
Copy-Item -Recurse -Force "$workspace_dir/examples" $dir 

Compress-Archive -Force -Path $dir -DestinationPath "$package_dir/hlsl_box.zip"

Write-Host "[package.ps1] finished"
