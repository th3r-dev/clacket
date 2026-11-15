// Language toggle for the Terms and Privacy pages. Both language blocks live in the
// page; CSS hides the one that isn't selected via [data-lang] on <html>. External file
// because the site's CSP is script-src 'self' (no inline scripts).
(function () {
  var root = document.documentElement;
  var buttons = document.querySelectorAll(".lang-btn");

  function set(lang) {
    root.setAttribute("data-lang", lang);
    root.setAttribute("lang", lang);
    buttons.forEach(function (b) {
      b.setAttribute("aria-pressed", String(b.dataset.set === lang));
    });
  }

  buttons.forEach(function (b) {
    b.addEventListener("click", function () { set(b.dataset.set); });
  });

  // default to the browser language, fall back to English
  var wantsThai = (navigator.language || "en").toLowerCase().indexOf("th") === 0;
  set(wantsThai ? "th" : "en");
})();
