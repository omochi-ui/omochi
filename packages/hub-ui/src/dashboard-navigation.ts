type Guard = () => Promise<boolean>;
interface Navigation {
  register(guard: Guard): () => void;
  requestLeave(): Promise<boolean>;
}
export function createDashboardNavigation(): Navigation {
  let guard: Guard | null = null;
  return {
    register(next) {
      guard = next;
      return () => {
        if (guard === next) guard = null;
      };
    },
    requestLeave() {
      return guard?.() ?? Promise.resolve(true);
    },
  };
}
