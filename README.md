# mdp

MarkDownPub: an easy command wrapper for pandoc.

## User story

Finish editing; run mdp.

Option A: mdp identifies that it is running in a directory containing a src folder. As a result, mdp presumes that the default input to pandoc will be whatever is in src and that the default name for the output(s) is whatever the parent directory is named.

Option B: mdp identifies that there is no src directory. In that case, it will expect to have been provided with a source directory ("path" in our opts struct) or it will try to find a config file (.mdp?) containing a list of jobs to perform.

In either case, the order of precedence is:

1. Explicit arguments passed at runtime
2. Configuration stored in the .mdp file
3. Convention

When running from a configuration file, mdp should be able to build multiple outputs at once. Additionally, it should only build those outputs which are out of date (which means the output of mdp should include the last modified time for constituent files). Imagine a structure similar to the following:

```
/target
    /book-name-one
        output.log # this name is far from set in stone
        book-name-one.docx
        book-name-one.epub
        book-name-one.pdf
    /book-name-two
        output.log
        book-name-two.docx
        book-name-two.html
```

> Note that our configuration somehow calls for the two books to have different outputs.

I'm thinking the configuration will need to be something slightly more human-readable than usual. Prefer toml over json.
