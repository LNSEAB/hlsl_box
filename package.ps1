$dir = "package/hlsl_box"

cargo.exe build --profile production

if (Test-Path -Path $dir) {
    Remove-Item -Recurse -Force -Path $dir
}
New-Item $dir -ItemType Directory
Copy-Item "dxcompiler.dll" $dir
Copy-Item "dxil.dll" $dir
Copy-Item "target/production/hlsl_box.exe" $dir
Copy-Item -Recurse -Force "include" $dir 
Compress-Archive -Force -Path $dir -DestinationPath "package/hlsl_box.zip"