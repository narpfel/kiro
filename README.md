Kiro
===

Kiro is an (in progress) Rust port of [Kilo](https://github.com/antirez/kilo),
a small text editor in less than 1K lines of code (counted with cloc).

A screencast is available here: https://asciinema.org/a/90r2i9bq8po03nazhqtsifksb

Usage: kiro `<filename>`

Keys:

    CTRL-S: Save
    CTRL-Q: Quit
    CTRL-F: Find string in file (ESC to exit search, arrows to navigate)

Kiro does not depend on any library (not even curses). It uses fairly standard
VT100 (and similar terminals) escape sequences. The project is in alpha
stage and was written in just a few hours taking code from my other two
projects, load81 and linenoise.

People are encouraged to use it as a starting point to write other editors
or command line interfaces that are more advanced than the usual REPL
style CLI.

Kiro was written under the name Kilo by Salvatore Sanfilippo aka antirez and is
released under the BSD 2 clause license. Its original GitHub repo can be found
[here](https://github.com/antirez/kilo).
