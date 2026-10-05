import { useCallback, useEffect, useState } from "react";

// Tiny hash router: #/  #/try  #/how  #/trace  #/trace/<action id>
export type Route = { page: "home" | "try" | "how" | "trace"; id?: string };

function parse(): Route {
  const [, page = "", id] = window.location.hash.replace(/^#/, "").split("/");
  if (page === "try" || page === "how" || page === "trace") return { page, id };
  return { page: "home" };
}

export function useRoute(): [Route, (to: string) => void] {
  const [route, setRoute] = useState<Route>(parse);
  useEffect(() => {
    const on = () => { setRoute(parse()); window.scrollTo({ top: 0 }); };
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  const go = useCallback((to: string) => { window.location.hash = to; }, []);
  return [route, go];
}
