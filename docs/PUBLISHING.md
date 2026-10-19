# Publishing the documentation

The documentation site is [laszukdawid.github.io/domowik](https://laszukdawid.github.io/domowik/).
Its source is the existing Markdown in `docs/`; the repository remains the
source of truth for editing and review.

## GitHub Pages settings

In **Settings → Pages → Build and deployment**, use:

- **Source:** Deploy from a branch
- **Branch:** `main`
- **Folder:** `/docs`

The owner selected these settings on 2026-10-18. Pushing a documentation
change to `main` lets GitHub's managed Pages build publish it. No custom
Actions workflow is required. The separate product CI remains manual;
Pages performs only the documentation build.

`_config.yml` sets the project URL and `/domowik` base path. Update both if
the owner, repository name or domain changes. `_layouts/default.html` and
`site/style.css` provide the original responsive layout, without a downloaded
theme, web fonts or a frontend build. The layout uses system fonts.

## Markdown, images and links

Keep writing ordinary Markdown. GitHub Pages' supported plugins render files
without front matter, use each directory's README as its index, and rewrite
links to published Markdown pages as HTML links. `docs/README.md` is the
homepage; `docs/decisions/README.md` is the decisions index.

Images stay in their existing relative locations. `site/links.js` adapts
repository-relative links that point outside `docs/`, such as `../examples/`,
to GitHub source/raw URLs. This keeps the same source
Markdown useful both in a checkout and on the website. Main navigation and
internal guide links work without JavaScript; **View this page on GitHub**
is also available if JavaScript is disabled. The example itself remains in
`examples/welcome-home/`, with its original models embedded in the download.

After changing the site, check the homepage, a guide, the decisions index,
images, demo links and a narrow viewport. Inspect the managed Pages build in
GitHub's Actions tab if publishing fails; an enabled setting alone does not
prove that a particular commit has deployed.

## Local preview

With Ruby/Jekyll installed, install the supported plugin versions in a local
gem environment:

```sh
gem install jekyll -v 3.10.0
gem install jekyll-optional-front-matter -v 0.3.2
gem install jekyll-readme-index -v 0.3.0
gem install jekyll-relative-links -v 0.6.1
gem install kramdown-parser-gfm -v 1.1.0
jekyll serve --source docs --destination /tmp/domowik-docs
```

Open `http://127.0.0.1:4000/domowik/`. Generated output goes outside the
checkout. GitHub controls its managed Jekyll/runtime updates; a local preview
is useful verification, not proof of a hosted deployment.

## Tool and content provenance

The layout, CSS and link adaptation script are original Apache-2.0 project
code. The three plugins above are MIT-licensed development/build tools from
`benbalter/jekyll-optional-front-matter`, `benbalter/jekyll-readme-index` and
`benbalter/jekyll-relative-links`; their versions match the GitHub Pages build
stack reviewed on 2026-10-18. Jekyll 3.10.0 and the local GFM parser 1.1.0 are MIT-licensed. No plugin or Jekyll
runtime is shipped inside Domowik binaries, and no new product dependency,
third-party theme, font or catalog is introduced. Documentation images retain
[their recorded provenance](images/README.md).

See GitHub's [publishing-source instructions](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)
and [Pages/Jekyll documentation](https://docs.github.com/en/pages/setting-up-a-github-pages-site-with-jekyll/about-github-pages-and-jekyll).
