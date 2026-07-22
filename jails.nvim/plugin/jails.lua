-- Auto-loaded from the runtimepath's plugin/ dir. Registers :Jails
-- regardless of filetype, so it's available anywhere inside a jails-
-- managed project.
if vim.g.loaded_jails then return end
vim.g.loaded_jails = true

vim.api.nvim_create_user_command('Jails', function(cmd_opts)
  require('jails').dispatch(cmd_opts.fargs)
end, {
  nargs = '+',
  desc = 'Run a jails subcommand: :Jails {new|new-cli|generate|g|destroy|d|test|build|run} ...',
  complete = function(_, line)
    return require('jails').complete(nil, line)
  end,
})
