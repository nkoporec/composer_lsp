# composer_lsp

The composer language server provides various features for composer to make development easier and faster.

![2022-11-10_17-28](https://user-images.githubusercontent.com/35064680/201152124-de141c8f-4446-478e-865c-0a08b79c4bd2.png)

## Debugging

For better debugging, you can use additional file logging with log4rs crate. A enviroment variable `COMPOSER_LSP_LOG` needs to be set, which points to the log4rs yaml config file. For more information check the log4rs documentation.

## Features

- [X] Shows when a package needs an update.
- [X] Package name hover, to show details about it.
- [X] Package go to definition.
- [X] Package name completion.
- [ ] Actions to update the selected package.

## Install

Using cargo

 `cargo install composer_lsp`

## Editor Setup

### Neovim

Plugins required:
 - lspconfig (https://github.com/neovim/nvim-lspconfig)

After installing the package, add this to your lua config

```lua
local configs = require 'lspconfig.configs'
local lspconfig = require 'lspconfig'
if not configs.composer_lsp then
 configs.composer_lsp = {
   default_config = {
     cmd = {'composer_lsp'},
     filetypes = {'json'},
     root_dir = function(pattern)
      local cwd = vim.loop.cwd()
      local root = lspconfig.util.root_pattern('composer.json', '.git')(pattern)

      -- prefer cwd if root is a descendant
      return lspconfig.util.path.is_descendant(cwd, root) and cwd or root
     end,
     settings = {},
   },
 }
end
lspconfig.composer_lsp.setup{}
```

### VS Code

TODO - Still need to build an extension for it.
