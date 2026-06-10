Drop captured `page.mu` files in this directory to include them in the Micron
renderer regression corpus.

Recommended naming:

```text
<node-or-page-name>-<short-hash>.mu
```

The test harness renders every `.mu` file under `fixtures/micron/` recursively
at 40, 60, 71, and 80 columns. Keep captures focused on renderer behavior:
color, alignment, links, controls, half-block art, wrapping, and syntax spill.
