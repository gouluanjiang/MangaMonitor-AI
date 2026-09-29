import { createContext, Fragment, useContext } from "react";
import "./browse-controls.css";

const AuthorSearchContext = createContext<((author: string) => void) | null>(
  null,
);
export const AuthorSearchProvider = AuthorSearchContext.Provider;

export function AuthorLinks({
  authors,
  fallback = "作者资料未取得",
  className = "",
}: {
  authors: readonly string[];
  fallback?: string;
  className?: string;
}) {
  const search = useContext(AuthorSearchContext);
  const names = [
    ...new Set(authors.map((author) => author.trim()).filter(Boolean)),
  ];
  return (
    <p
      className={`author-links ${className}`}
      title={names.join("、") || fallback}
    >
      {names.length
        ? names.map((name, index) => (
            <Fragment key={name}>
              {index > 0 && <span aria-hidden="true">、</span>}
              {search ? (
                <button
                  type="button"
                  className="browse-author-link"
                  onClick={(event) => {
                    event.stopPropagation();
                    search(name);
                  }}
                  aria-label={`搜索作者 ${name}`}
                  title={name}
                >
                  {name}
                </button>
              ) : (
                <span>{name}</span>
              )}
            </Fragment>
          ))
        : fallback}
    </p>
  );
}
