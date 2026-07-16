// ------------------------------------------------------------
// SSE connection — reconnect with backoff, refetch on visibility
// ------------------------------------------------------------
const sse = {
  es: null,
  connected: false,
  backoff: 1000,
  onFleetEvent: null,
  onSessionEvent: null,
  onStateChange: null,

  connect() {
    if (this.es) {
      try { this.es.close(); } catch (_) {}
    }
    let es;
    try {
      es = new EventSource('/api/events');
    } catch (_) {
      this.scheduleReconnect();
      return;
    }
    this.es = es;
    es.onopen = () => {
      this.connected = true;
      this.backoff = 1000;
      if (this.onStateChange) this.onStateChange(true);
    };
    es.onerror = () => {
      this.connected = false;
      if (this.onStateChange) this.onStateChange(false);
      try { es.close(); } catch (_) {}
      this.scheduleReconnect();
    };
    es.onmessage = (ev) => {
      let data;
      try { data = JSON.parse(ev.data); } catch (_) { return; }
      if (!data || typeof data !== 'object') return;
      if (data.type === 'fleet') {
        if (this.onFleetEvent) this.onFleetEvent();
      } else if (data.type === 'session') {
        if (this.onSessionEvent) this.onSessionEvent(data.pane_id);
      }
    };
  },

  scheduleReconnect() {
    setTimeout(() => this.connect(), this.backoff);
    this.backoff = Math.min(this.backoff * 2, 15000);
  },
};

export { sse };
