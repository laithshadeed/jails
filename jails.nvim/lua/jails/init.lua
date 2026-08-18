-- jails.nvim: a thin, project-aware Neovim wrapper around the `jails` CLI.
-- The CLI remains the source of truth; this module only supplies editor UX.
local M = {}

local config = {
  command = 'jails',
  terminal_height = 15,
  open_created = true,
  root_markers = { 'pom.xml' },
}

local STREAMING = {
  test = true,
  build = true,
  run = true,
  check = true,
  fmt = true,
  mvn = true,
}

local KINDS = {
  'scaffold',
  'controller',
  'service',
  'class',
  'interface',
  'record',
  'value',
  'enum',
  'sealed',
  'repo',
  'migration',
  'handler',
  'command',
  'cli',
  'cases',
  'test',
  'integration-test',
}

local CAPABILITIES = { 'db', 'csv', 'sqlite', 'json', 'testkit', 'fake', 'http', 'format' }

local SUBCOMMANDS = {
  'new',
  'new-cli',
  'generate',
  'g',
  'add',
  'a',
  'destroy',
  'd',
  'test',
  'build',
  'fmt',
  'check',
  'mvn',
  'run',
  'completion',
  'help',
}

local OPTIONS = {
  new = { '--deps', '--java', '--no-git', '--no-devtools' },
  ['new-cli'] = { '--release', '--no-git' },
  add = { '--name', '--dry-run', '--package' },
  a = { '--name', '--dry-run', '--package' },
  destroy = { '--force', '--package' },
  d = { '--force', '--package' },
  generate = { '--package' },
  g = { '--package' },
  run = { '--no-build', '--watch', '--' },
}

function M.setup(opts)
  config = vim.tbl_deep_extend('force', config, opts or {})
end

function M.project_root()
  local buffer_root = vim.b.jails_root
  if buffer_root and buffer_root ~= '' then return buffer_root end

  local name = vim.api.nvim_buf_get_name(0)
  local start = name ~= '' and vim.fs.dirname(name) or vim.fn.getcwd()
  return vim.fs.root(start, config.root_markers) or vim.fn.getcwd()
end

local function jails_bin()
  if vim.fn.executable(config.command) == 0 then
    vim.notify(
      ('jails.nvim: `%s` not found on PATH'):format(config.command),
      vim.log.levels.ERROR
    )
    return nil
  end
  return config.command
end

local function trim(value)
  return (value or ''):gsub('%s+$', '')
end

local function absolute_path(root, path)
  if path:sub(1, 1) == '/' or path:match('^%a:[/\\]') then return path end
  return vim.fs.joinpath(root, path)
end

local function created_files(output, root)
  local files = {}
  local seen = {}

  for _, line in ipairs(vim.split(output, '\n', { plain = true })) do
    local path
    local root_at = line:find(root, 1, true)
    if root_at then
      path = line:sub(root_at)
    else
      path = line:match('^%s*create%s+(.+)$')
    end

    if path then
      path = vim.fs.normalize(absolute_path(root, path))
      if not path:match('/%.gitkeep$') and vim.fn.filereadable(path) == 1 and not seen[path] then
        seen[path] = true
        table.insert(files, path)
      end
    end
  end

  return files
end

local function open_created_files(output, root)
  local files = created_files(output, root)
  if #files == 0 then return end

  local items = {}
  for _, path in ipairs(files) do
    table.insert(items, { filename = path, lnum = 1, col = 1 })
  end
  vim.fn.setqflist({}, 'r', { title = 'Jails created files', items = items })
  vim.cmd.edit(vim.fn.fnameescape(files[1]))

  if #files > 1 then
    vim.notify(
      ('jails.nvim: opened %s; %d generated files are in the quickfix list (:cnext)'):format(
        vim.fn.fnamemodify(files[1], ':t'),
        #files
      ),
      vim.log.levels.INFO
    )
  end
end

--- Run a short command asynchronously from the nearest Maven project.
function M.run(args, opts)
  local bin = jails_bin()
  if not bin then return end

  opts = opts or {}
  local root = opts.cwd or M.project_root()
  local cmd = { bin }
  vim.list_extend(cmd, args)

  return vim.system(cmd, { cwd = root, text = true }, function(result)
    vim.schedule(function()
      local stdout = trim(result.stdout)
      local stderr = trim(result.stderr)
      if result.code ~= 0 then
        local detail = stderr ~= '' and stderr or stdout
        vim.notify(
          ('jails %s failed%s'):format(
            table.concat(args, ' '),
            detail ~= '' and (':\n' .. detail) or ''
          ),
          vim.log.levels.ERROR
        )
        return
      end

      if stdout ~= '' and opts.notify ~= false then
        vim.notify(stdout, vim.log.levels.INFO)
      end
      if config.open_created and opts.open_created ~= false then
        open_created_files(stdout, root)
      end
    end)
  end)
end

local term_win

--- Run a long command in one reusable bottom terminal, rooted at pom.xml.
function M.run_terminal(args)
  local bin = jails_bin()
  if not bin then return end

  local root = M.project_root()
  local cmd = { bin }
  vim.list_extend(cmd, args)

  if term_win and vim.api.nvim_win_is_valid(term_win) then
    vim.api.nvim_set_current_win(term_win)
  else
    vim.cmd('botright split')
    term_win = vim.api.nvim_get_current_win()
  end
  vim.api.nvim_win_set_height(term_win, config.terminal_height)

  local buffer = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_win_set_buf(term_win, buffer)
  vim.bo[buffer].bufhidden = 'wipe'
  vim.bo[buffer].swapfile = false
  vim.b[buffer].jails_root = root

  vim.fn.termopen(cmd, {
    cwd = root,
    on_exit = function(_, code)
      if code ~= 0 then
        vim.schedule(function()
          vim.notify(('jails %s exited with status %d'):format(args[1] or '', code), vim.log.levels.ERROR)
        end)
      end
    end,
  })
  vim.cmd.startinsert()
end

function M.destroy(args)
  if #args < 2 then
    vim.notify('jails.nvim: usage :Jails destroy <kind> <Name>', vim.log.levels.ERROR)
    return
  end

  local choice = vim.fn.confirm(('destroy %s?'):format(table.concat(args, ' ')), '&Yes\n&No', 2)
  if choice ~= 1 then return end

  local command = { 'destroy' }
  vim.list_extend(command, args)
  if not vim.tbl_contains(command, '--force') then table.insert(command, '--force') end
  M.run(command)
end

function M.dispatch(fargs)
  if #fargs == 0 then
    vim.notify(
      'jails.nvim: :Jails <g|a|d|test|check|run> ... (use :Jails help for everything)',
      vim.log.levels.INFO
    )
    return
  end

  local sub = fargs[1]
  if sub == 'destroy' or sub == 'd' then
    M.destroy(vim.list_slice(fargs, 2))
  elseif STREAMING[sub] then
    M.run_terminal(fargs)
  else
    M.run(fargs)
  end
end

local function matching(values, lead)
  if not lead or lead == '' then return values end
  local matches = {}
  for _, value in ipairs(values) do
    if value:sub(1, #lead) == lead then table.insert(matches, value) end
  end
  return matches
end

local function test_names()
  local root = M.project_root()
  local dir = vim.fs.joinpath(root, 'src/test/java')
  if vim.fn.isdirectory(dir) == 0 then return {} end

  local names = {}
  local seen = {}
  for _, path in ipairs(vim.fs.find(function(name)
    return name:match('Tests?%.java$') or name:match('IT%.java$')
  end, { path = dir, type = 'file', limit = 500 })) do
    local name = vim.fn.fnamemodify(path, ':t:r')
    if not seen[name] then
      seen[name] = true
      table.insert(names, name)
    end
  end
  table.sort(names)
  return names
end

--- Complete subcommands, generator kinds, capabilities, options and test names.
function M.complete(arg_lead, cmd_line)
  local words = vim.split(cmd_line, '%s+', { trimempty = true })
  table.remove(words, 1) -- :Jails
  local position = #words + (cmd_line:match('%s$') and 1 or 0)

  if position <= 1 then return matching(SUBCOMMANDS, arg_lead) end

  local sub = words[1]
  if position == 2 then
    if sub == 'generate' or sub == 'g' or sub == 'destroy' or sub == 'd' then
      return matching(KINDS, arg_lead)
    end
    if sub == 'add' or sub == 'a' then
      return matching(CAPABILITIES, arg_lead)
    end
    if sub == 'test' then return matching(test_names(), arg_lead) end
  end

  return matching(OPTIONS[sub] or {}, arg_lead)
end

return M
