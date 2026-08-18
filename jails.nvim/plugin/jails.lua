-- Auto-loaded from the runtimepath's plugin/ dir. Registers :Jails
-- regardless of filetype, so it's available anywhere inside a jails-
-- managed project.
if vim.g.loaded_jails then return end
vim.g.loaded_jails = true

vim.api.nvim_create_user_command('Jails', function(cmd_opts)
  require('jails').dispatch(cmd_opts.fargs)
end, {
  nargs = '*',
  desc = 'Run jails in the Maven project containing the current buffer',
  complete = function(arg_lead, line)
    return require('jails').complete(arg_lead, line)
  end,
})

-- Hot-reload on save: source lives in the jails repo, edited directly, and
-- Lua caches `require`d modules -- without this, a change to init.lua
-- wouldn't take effect until Neovim restarts.
local plugin_root = vim.fn.fnamemodify((debug.getinfo(1, 'S').source):sub(2), ':p:h:h')
vim.api.nvim_create_autocmd('BufWritePost', {
  pattern = {
    plugin_root .. '/lua/jails/*.lua',
    plugin_root .. '/plugin/jails.lua',
  },
  callback = function()
    package.loaded.jails = nil
    vim.notify('jails.nvim: reloaded', vim.log.levels.INFO)
  end,
  desc = 'Reload jails.nvim after editing its own source',
})
