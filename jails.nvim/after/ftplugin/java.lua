-- Project source navigation and compiler integration belong to the Java
-- buffer, not to a global Neovim config. Neovim's stock Java ftplugin already
-- supplies includeexpr/suffixesadd/include/define; jails fills the missing
-- search roots and keeps build commands on the uppercase key namespace.
require('jails').configure_java_buffer()
