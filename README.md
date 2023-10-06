# fjf — A Simple GitHub Checks CLI

`fjf` lets you check the status of your GitHub check runs for the current git ref:

```bash
> fjf status
Found 1 runs for main

build   🟢
```

It attempts to detect the GitHub repository from your origin remote url, but if it can't, you can manually supply 
the repository owner and name:

```bash
fjf status --owner NicholasLYang --repo vicuna
```

You can also open a check run in your browser:

```bash
fjf open
```

If you want to use `fjf` on private repositories, you can login to GitHub:

```bash
fjf login
```

## Why Does This Exist?

`gh`, GitHub's official CLI, does allow you to list check runs, but only in the context of pull requests. Also, it 
requires you to either look up your PR number or remember it. That seemed like unnecessary friction to me.

I got sick of refreshing my browser to see if my check runs had finished, so I wrote this.

