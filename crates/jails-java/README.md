# `jails-java`

Reading Java and rendering it.

- `java` -- a deliberately small reader: annotations and what they attach to, a
  type's supertypes, a constructor's parameters. Not a parser and must not grow
  into one.
- `classfile` -- the smallest reader that answers "which types does this class
  name": constant pool only. `CONSTANT_Long` and `CONSTANT_Double` take two
  pool slots.
- `template` -- `{{name}}` substitution into real `.java` files. A missing or
  unused key is a panic. Substitution only, never a template engine.
