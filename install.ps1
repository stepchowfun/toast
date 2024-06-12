# Follows the steps from https://github.com/stepchowfun/toast/tree/9212b25daf193bb5d372d3adbdc0637e2c1d2ff9#installation-on-windows-x86-64

# Opens window asking for admin privileges if needed.
# https://stackoverflow.com/a/63344749/12756474
if(!([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] 'Administrator')) {
  Start-Process -FilePath PowerShell.exe -Verb Runas -ArgumentList "-File `"$($MyInvocation.MyCommand.Path)`"  `"$($MyInvocation.MyCommand.UnboundArguments)`""
  Exit
}

set-location $PSScriptRoot

# We want to get the latest version
$BaseUrl = "https://github.com/stepchowfun/toast/releases/latest";
$Response = [System.Net.WebRequest]::Create($BaseUrl).GetResponse()

$ExecutableUrl = $Response.ResponseUri.OriginalString + "/toast-x86_64-pc-windows-msvc.exe"  -replace "tag", "download"

$Response.Close()
$Response.Dispose()

$ToastPath = "$env:PROGRAMFILES/Toast"

# If the directory doesn't exist, create it
if (!(Test-Path $ToastPath)) {
  mkdir $ToastPath
}

# Could do the same for the exe but chose not to for the sake of a reinstall or an update
# if (!Test-Path 'C:\Program Files\Toast\toast.exe') {
#   Invoke-WebRequest $ExecutableUrl -OutFile "$env:PROGRAMFILES/Toast/toast.exe"
# }

# Download file
Invoke-WebRequest $ExecutableUrl -OutFile "$ToastPath/toast.exe"

# Add to path (apparently duplicates are not allowed in the PATH so no reason to check if it is already there)
$OldPath = [System.Environment]::GetEnvironmentVariable("Path")
[System.Environment]::SetEnvironmentVariable("Path", $OldPath + ';' + $ToastPath, [System.EnvironmentVariableTarget]::Machine)