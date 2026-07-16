(() => {
  const dialog = document.querySelector('.msk-zoom');
  const image = document.querySelector('.msk-zoom-img');
  const title = document.querySelector('#zoom-title');
  const close = document.querySelector('[data-zoom-close]');
  if (!(dialog instanceof HTMLDialogElement) || !image || !title || !close) {
    return;
  }

  document.querySelectorAll('.msk-shot').forEach((button) => {
    button.addEventListener('click', () => {
      const src = button.getAttribute('data-full');
      const shotTitle = button.getAttribute('data-title') || 'Screenshot';
      if (!src) return;
      image.setAttribute('src', src);
      image.setAttribute('alt', `${shotTitle} enlarged screenshot`);
      title.textContent = shotTitle;
      dialog.showModal();
    });
  });

  close.addEventListener('click', () => dialog.close());
  dialog.addEventListener('click', (event) => {
    if (event.target === dialog) {
      dialog.close();
    }
  });
})();
