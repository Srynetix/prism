//! `#[command]`, on each command of `prism-commands`: the function registered with Tauri (the
//! window's app, feature `tauri`), and beside it a module of the same name whose `peer` answers
//! the command from JSON, its arguments named as the window sends them (camelCase) — what another
//! Prism's window calls over its socket (`commands::peer_call`).

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, Pat, ReturnType, Type};

#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    // `#[command(async)]`: Tauri's own options, passed on.
    let attr = proc_macro2::TokenStream::from(attr);
    let tauri = if attr.is_empty() { quote! { tauri::command } } else { quote! { tauri::command(#attr) } };
    let f = parse_macro_input!(item as ItemFn);
    let name = &f.sig.ident;
    let mut fields = Vec::new();
    let mut call = Vec::new();
    for input in &f.sig.inputs {
        let FnArg::Typed(arg) = input else {
            return syn::Error::new_spanned(input, "a command takes no self").to_compile_error().into();
        };
        let Pat::Ident(ident) = &*arg.pat else {
            return syn::Error::new_spanned(&arg.pat, "a command's argument is a plain name").to_compile_error().into();
        };
        let ident = &ident.ident;
        if is_state(&arg.ty) {
            call.push(quote! { crate::commands::State(app) });
        } else {
            let ty = &arg.ty;
            fields.push(quote! { #ident: #ty });
            call.push(quote! { a.#ident });
        }
    }
    let awaited = if f.sig.asyncness.is_some() { quote! { .await } } else { quote! {} };
    // A command answers a `Cmd<T>` (a `Result`), or its value as it is.
    let result = match &f.sig.output {
        ReturnType::Type(_, ty) if returns_result(ty) => quote! { crate::commands::reply(answer) },
        ReturnType::Default => quote! { { let () = answer; Ok(serde_json::Value::Null) } },
        ReturnType::Type(..) => quote! { Ok(serde_json::to_value(answer)?) },
    };
    let module = format_ident!("{}", name);
    quote! {
        #[cfg_attr(feature = "tauri", #tauri)]
        #f

        #[doc(hidden)]
        #[allow(non_snake_case, clippy::all)]
        pub mod #module {
            #[allow(unused_imports)]
            use super::*;

            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            #[allow(dead_code)]
            struct Args { #(#fields),* }

            /// The command called with JSON arguments, its answer as JSON.
            pub fn peer(app: &crate::app::AppState, args: serde_json::Value) -> futures::future::BoxFuture<'_, anyhow::Result<serde_json::Value>> {
                Box::pin(async move {
                    let a: Args = crate::commands::args(args)?;
                    let _ = &a;
                    let answer = super::#name(#(#call),*) #awaited;
                    #result
                })
            }
        }
    }
    .into()
}

/// `State<'_>` / `State<'_, AppState>`: the state, not an argument.
fn is_state(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "State"))
}

fn returns_result(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Cmd" || s.ident == "Result"))
}
