# Shipped pages

The pages that ship, for every screen, one file per module, named after its
catalogue key the way profiles are named (`FA-18C_hornet` is
`fa-18c-hornet.json`). A profile's page slots point at pages here by id.

Installed, these are copied into the page library in the user's data folder
and reconciled on each update; in a development checkout this folder is the
library itself. See docs/CONFIG.md, "Pages".

Only the `.json` files are read.
