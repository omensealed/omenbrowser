#!c=0
>MicronPlus Columns Fixture

[window title="Live Dashboard"]
[columns]
[column title="Controls" weight=3]
[status text="MicronPlus active" style="success"]
[live id="feed" src=":/page/feed.mu" refresh=3 loop=2 fields="message"]
[textbox name="message" label="Message" width=28 value="hello"]
[button label="Refresh" action="p:feed:log" fields="message"]
[/column]
[column title="Recent" weight=2]
[scrollbox title="Scroll" height=2]
row 1
row 2
row 3
[/scrollbox]
[log id="log" height=2 max=3]
old
new
latest
[/log]
[/column]
[/columns]
[/window]
