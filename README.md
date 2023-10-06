# fj — A Simple GitHub Checks CLI

`fj` lets you check the status of your GitHub check runs for the current git ref:

```bash
fj status
```

It attempts to detect the GitHub repository from your origin remote url, but if it can't, you can manually supply 
the repository owner and name:

```bash
fj status --owner NicholasLYang --repo fj
```

You can also open a check run in your browser:

```bash
fj open
```

If you want to use `fj` on private repositories, you can login:

```bash
fj login
```

## Why Does This Exist?

`gh`, GitHub's official CLI, does allow you to list check runs, but only in the context of pull requests. Also, it 
requires you to either look up your PR number or remember it. That seemed like unnecessary friction to me.

I got sick of refreshing my browser to see if my check runs had finished, so I wrote this.

