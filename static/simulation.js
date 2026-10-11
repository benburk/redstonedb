// A build's simulation, run in the background once it is published: while
// it runs, its page asks the server every few seconds and shows the result
// when it is in. 202 means still running, 200 brings the section, and 204
// means there is nothing to wait for. A page that was waiting for the cases
// themselves reloads, since whether the build fails shows above them too.
for (const pending of document.querySelectorAll("[data-poll]")) {
  const poll = async () => {
    try {
      const response = await fetch(pending.dataset.poll);
      if (response.status === 200) {
        if (pending.hasAttribute("data-poll-reload")) {
          location.reload();
        } else {
          pending.closest("#simulation").outerHTML = await response.text();
        }
        return;
      }
      if (response.status === 204) {
        pending.querySelector('[role="status"]').textContent =
          "The result changed. Reload the page to see it.";
        return;
      }
    } catch {
      // A failed request is asked again with the rest.
    }
    setTimeout(poll, 3000);
  };
  setTimeout(poll, 3000);
}
