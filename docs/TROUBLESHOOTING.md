# Troubleshooting

## Browser Does Not Connect

- Check the selected identity.
- Check Interfaces configuration.
- Confirm the gateway is reachable.
- Use Diagnostics for the target destination.
- Check Logs for path/link/request status.

## NomadNet Page Times Out

- Request path for the node and wait for path success.
- Retry the URL after the path is known.
- Confirm the URL uses the expected `destination:/page/index.mu` style.
- Some larger form submits may be limited by current Rust RNS crate behavior.

## OMENchat Does Not Connect

- Confirm the server is running.
- Confirm the client knows or can request the server path.
- Use the Reconnect button after path success.
- Check `omenchatd` Monitoring and Logs.
- Check OMENbrowser_rs Monitoring for chat link state.

## Media Does Not Load Inline

- Check whether the media is clearweb HTTP/HTTPS or Reticulum/NomadNet.
- For clearweb images, check SOCKS5/Tor detection and media privacy settings.
- Confirm the file is below server upload limits.
- Use the media action button to retry/download.

## Reporting Issues

Use:

```bash
bash scripts/alpha-collect.sh \
  --browser-root <browser-root> \
  --server-home <server-home>
```

Review the output before sharing it.
