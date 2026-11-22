# Python bridge

`freeplots.py` runs the FreePlots command line from Python and reads the
Session text back: no Python package, no server, only `subprocess` and the
standard library.

```python
import os
os.environ["FREEPLOTS_BIN"] = "/path/to/FreePlots"
from freeplots import run, statistic

session = run("ttest1", "weights.csv", "--column", "weight", "--mean", "9.5")
print(statistic(session, "Estimate"), statistic(session, "t"))
```

`freeplots.py` offers:

- `run(*args)` — runs one command and returns its Session text; a failing
  command raises `RuntimeError` with the executable's stderr;
- `statistic(session, label)` — the value of a `label   value` statistics line;
- `table_row` / `table_value` — the cells of a table row, such as the `Cp` row
  of a capability study.

`example_descriptive.py` is a complete example: it prints `mean=… sd=… t=…` for
a CSV and column. Run it with the executable on `FREEPLOTS_BIN` or on `PATH`.

The bridge is the supported way to script FreePlots from Python: it uses the
documented command line (see the executable's `help`) and the documented
Session layout, so upgrades that keep both keep the bridge working.
