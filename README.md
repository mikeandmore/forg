# Forg: File Organizer

Forg is a file manager inspired by Emacs Dired. It has a modernized, icon-based UI, but it also provides efficient keyboard-oriented user interaction. Just like Emacs, Forg provides different key bindings under different modes.

Under main view:

| Key               | Action                                            |
|-------------------|---------------------------------------------------|
| `n`               | Move to the next item.                            |
| `p`               | Move to the previous item.                        |
| `alt-<`           | Move to the first item.                           |
| `alt->`           | Move to the last item.                            |
| `m`               | Mark the current item.                            |
| `h`               | Toggle hidden files/directories.                  |
| `d`               | Delete current item or marked items.              |
| `r`               | Rename current item. This enters the rename mode. |
| `enter`           | Open the current file or directory.               |
| `backspace`       | Go back in history.                               |
| `^`               | Go to the parent directory.                       |
| `ctrl-s`          | Enter search mode. Or search the next item.       |
| `escape`/`ctrl-g` | Exit the search/rename mode.                      |
| `ctrl-w`          | Cut current item or marked items.                 |
| `alt-w`           | Copy current item or marked items.                |
| `ctrl-y`          | Paste previously cut or copied items.             |
| `shift-n`         | Open a new window.                                |

Under search/rename mode:

An input box will pop up at the bottom of the window. In that input box, you can enter search keywords or new names. Here are the key bindings supported in the input box:

| Key                 | Action                                        |
|---------------------|-----------------------------------------------|
| `backspace`         | Delete the previous character.                |
| `alt-backspace`     | Delete the previous word.                     |
| `delete`/`ctrl-d`   | Delete the next character.                    |
| `left`/`ctrl-b`     | Go to the previous character.                 |
| `alt-left`/`alt-b`  | Go to the previous word.                      |
| `right`/`ctrl-f`    | Go to the next character.                     |
| `alt-right`/`alt-f` | Go to the next word.                          |
| `ctrl-x h`          | Select all.                                   |
| `home`/`ctrl-a`     | Go to the beginning of the input.             |
| `end`/`ctrl-e`      | Go to the end of the input.                   |
| `escape`/`ctrl-g`   | End selection or exit the search/rename mode. |
| `ctrl-space`        | Start selection.                              |
| `enter`             | Commit the text in the input.                 |

## Design

Forg is fast. When the user presses a key, Forg responds immediately. To do this, Forg never blocks the main UI thread: it always spawns a background worker thread for blocking operations. Forg is written in Rust with GPUI. You will need accelerated graphics.

Currently, Forg only supports Linux. OSX support is still WIP.
