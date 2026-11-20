// Appends the affiliation notice to the bottom of every page.
(function () {
  function add() {
    var m = document.querySelector("main");
    if (!m || m.querySelector(".sk-notice")) return;
    var hr = document.createElement("hr");
    var p = document.createElement("p");
    p.className = "sk-notice";
    p.style.cssText = "font-size:0.8em;opacity:0.7";
    p.textContent =
      "Independent tool. Not affiliated with or endorsed by Interactive Brokers LLC.";
    m.appendChild(hr);
    m.appendChild(p);
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", add);
  } else {
    add();
  }
})();
