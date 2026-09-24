# Shipped pages

The pages that ship, for every screen, one file per module, named by its catalogue key
(`FA-18C_hornet.json`). A profile's page slots point at pages here by id.

Installed, these are copied into the page library in the user's data folder
and reconciled on each update; in a development checkout this folder is the
library itself. See docs/CONFIG.md, "Pages".

Only the `.json` files are read.
