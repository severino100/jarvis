# Reads the Outlook (classic) calendar through COM and prints JSON. Used by the agenda (agenda.rs).
# Args: -Days <n> (from today 00:00). Read-only: never creates or changes items.
param([int]$Days = 7, [string]$StartDate = "")
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
try {
  $outlook = New-Object -ComObject Outlook.Application
  $ns = $outlook.GetNamespace('MAPI')
  $cal = $ns.GetDefaultFolder(9)
  $items = $cal.Items
  $items.IncludeRecurrences = $true
  $items.Sort('[Start]')
  $from = if ($StartDate) { [datetime]::ParseExact($StartDate, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture) } else { (Get-Date).Date }
  $to = $from.AddDays($Days)
  $fmt = 'g'
  $filter = "[Start] < '" + $to.ToString($fmt) + "' AND [End] > '" + $from.ToString($fmt) + "'"
  $found = $items.Restrict($filter)
  $out = @()
  foreach ($a in $found) {
    if ($a.Start -ge $to) { break }
    $body = ''
    try { $body = [string]$a.Body } catch {}
    $join = ''
    $m = [regex]::Match($body, 'https://teams\.microsoft\.com/l/meetup-join/[^\s>"<]+')
    if ($m.Success) { $join = $m.Value }
    $out += [pscustomobject]@{
      id        = [string]$a.GlobalAppointmentID + '|' + $a.Start.ToString('o')
      subject   = [string]$a.Subject
      start     = $a.Start.ToString('o')
      end       = $a.End.ToString('o')
      allDay    = [bool]$a.AllDayEvent
      location  = [string]$a.Location
      organizer = [string]$a.Organizer
      busy      = [int]$a.BusyStatus
      response  = [int]$a.ResponseStatus
      teams     = $join
    }
  }
  @{ ok = $true; account = [string]$ns.CurrentUser.Name; events = $out } | ConvertTo-Json -Depth 4 -Compress
} catch {
  @{ ok = $false; error = $_.Exception.Message } | ConvertTo-Json -Compress
}
