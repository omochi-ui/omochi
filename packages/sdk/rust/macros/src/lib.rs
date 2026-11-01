use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Data, DeriveInput, Fields, Meta, Token, parse_macro_input, parse_quote, punctuated::Punctuated,
};

#[proc_macro_derive(ActionInput, attributes(action_input))]
pub fn derive_input(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
fn adapter(attrs: &[syn::Attribute]) -> syn::Result<Option<syn::Path>> {
    let mut result = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("action_input")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("adapter") {
                result = Some(meta.value()?.parse()?);
                Ok(())
            } else {
                Err(meta.error("expected adapter = Path"))
            }
        })?;
    }
    Ok(result)
}
fn serde_name(attrs: &[syn::Attribute], key: &str) -> syn::Result<Option<String>> {
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        for meta in attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)? {
            if meta.path().is_ident(key) {
                if let Meta::List(ref list) = meta {
                    for nested in
                        list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
                    {
                        if let Meta::NameValue(nv) = nested {
                            if nv.path.is_ident("serialize") {
                                if let syn::Expr::Lit(lit) = nv.value {
                                    if let syn::Lit::Str(s) = lit.lit {
                                        return Ok(Some(s.value()));
                                    }
                                }
                            }
                        }
                    }
                }
                if let Meta::NameValue(nv) = meta {
                    if let syn::Expr::Lit(lit) = nv.value {
                        if let syn::Lit::Str(s) = lit.lit {
                            return Ok(Some(s.value()));
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}
fn serde_flag(attrs: &[syn::Attribute], key: &str) -> syn::Result<bool> {
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        for meta in attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)? {
            if meta.path().is_ident(key) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn skip_serializing(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    Ok(serde_flag(attrs, "skip")? || serde_flag(attrs, "skip_serializing")?)
}

/// Field types and bounds keep referring to the original type in mirrors and helpers.
struct ResolveSelf(syn::TypePath);
impl VisitMut for ResolveSelf {
    // Items inside const expressions introduce their own Self scope.
    fn visit_item_mut(&mut self, _: &mut syn::Item) {}

    fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
        visit_mut::visit_type_path_mut(self, ty);
        if ty.qself.is_none() && ty.path.segments.first().is_some_and(|s| s.ident == "Self") {
            if ty.path.segments.len() == 1 {
                *ty = self.0.clone();
            } else {
                let original = &self.0;
                let tail = syn::Path {
                    leading_colon: None,
                    segments: ty.path.segments.iter().skip(1).cloned().collect(),
                };
                *ty = parse_quote!(<#original>::#tail);
            }
        }
    }

    fn visit_expr_path_mut(&mut self, expr: &mut syn::ExprPath) {
        visit_mut::visit_expr_path_mut(self, expr);
        if expr.qself.is_none()
            && expr.path.segments.len() > 1
            && expr
                .path
                .segments
                .first()
                .is_some_and(|s| s.ident == "Self")
        {
            let original = &self.0;
            let tail = syn::Path {
                leading_colon: None,
                segments: expr.path.segments.iter().skip(1).cloned().collect(),
            };
            *expr = parse_quote!(<#original>::#tail);
        }
    }
}
fn rename_field(name: &str, rule: &str) -> String {
    match rule {
        "lowercase" | "snake_case" => name.into(),
        "UPPERCASE" | "SCREAMING_SNAKE_CASE" => name.to_uppercase(),
        "kebab-case" => name.replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => name.replace('_', "-").to_uppercase(),
        "PascalCase" | "camelCase" => {
            let mut s = name
                .split('_')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        Some(first) => first.to_uppercase().to_string() + c.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<String>();
            if rule == "camelCase" && !s.is_empty() {
                s.replace_range(..1, &s[..1].to_lowercase());
            }
            s
        }
        _ => name.into(),
    }
}
fn expand(mut input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (_, types, _) = input.generics.split_for_impl();
    ResolveSelf(parse_quote!(#name #types)).visit_derive_input_mut(&mut input);
    let name = &input.ident;
    let generated = format_ident!("{}Input", name.unraw());
    if let Some(adapter) = adapter(&input.attrs)? {
        let (ig, tg, wc) = input.generics.split_for_impl();
        let vis = &input.vis;
        return Ok(quote! {
            #vis type #generated #ig #wc = ::omochi_ui::AdaptedInput<#name #tg, #adapter>;
            impl #ig ::omochi_ui::ActionInput for #name #tg #wc { type Expression = #generated #tg; }
        });
    }
    for key in ["into", "remote"] {
        if serde_name(&input.attrs, key)?.is_some() {
            return Err(syn::Error::new_spanned(
                name,
                "custom container serialization requires #[action_input(adapter = Path)]",
            ));
        }
    }
    let rename_all = serde_name(&input.attrs, "rename_all")?.unwrap_or_default();
    let transparent = input.attrs.iter().any(|a| {
        a.path().is_ident("serde")
            && a.meta
                .require_list()
                .map(|m| m.tokens.to_string().contains("transparent"))
                .unwrap_or(false)
    });
    let mut mirror = input.clone();
    mirror.ident = generated.clone();
    mirror.attrs.retain(|a| a.path().is_ident("serde"));
    if serde_name(&input.attrs, "rename")?.is_none() {
        let wire_name = name.unraw().to_string();
        mirror
            .attrs
            .push(parse_quote!(#[serde(rename(serialize = #wire_name))]));
    }
    mirror
        .attrs
        .insert(0, parse_quote!(#[derive(::serde::Serialize)]));
    let mut projections = Vec::new();
    let mut predicates = Vec::new();
    let mut all_types = Vec::new();
    let mut count = 0usize;
    let (helper_generics, _, helper_where) = input.generics.split_for_impl();
    // Function lifetimes are inferred; explicitly passing late-bound lifetimes is invalid.
    let helper_args: Vec<_> = input
        .generics
        .params
        .iter()
        .filter_map(|p| match p {
            syn::GenericParam::Type(t) => {
                let id = &t.ident;
                Some(quote!(#id))
            }
            syn::GenericParam::Const(c) => {
                let id = &c.ident;
                Some(quote!(#id))
            }
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect();
    let mut transform = |fields: &mut Fields,
                         project: bool,
                         rule: &str,
                         tagged: bool,
                         skip: bool| {
        let newtype = matches!(fields, Fields::Unnamed(f) if f.unnamed.len() == 1);
        let mut serialized_index = 0usize;
        let mut stable_position = true;
        for (index, field) in fields.iter_mut().enumerate() {
            let ty = field.ty.clone();
            if skip {
                // Keep the variant constructible, without bounds, adapters, or helpers for its payload.
                field.attrs.retain(|a| a.path().is_ident("serde"));
                field.ty = parse_quote!(::omochi_ui::Input<#ty>);
                continue;
            }
            let adapter = adapter(&field.attrs)?;
            let helper_ty = if let Some(a) = &adapter {
                quote!(::omochi_ui::AdaptedInput<#ty, #a>)
            } else {
                quote!(::omochi_ui::Input<#ty>)
            };

            let mut wire = field
                .ident
                .as_ref()
                .map(|x| x.unraw().to_string())
                .unwrap_or(index.to_string());
            wire = serde_name(&field.attrs, "rename")?.unwrap_or_else(|| rename_field(&wire, rule));
            let mut flatten = newtype || transparent;
            let mut skipped = false;
            // Internally tagged newtypes merge their payload into the tag object.
            let mut real_flatten = tagged && newtype;
            let mut conditional = false;
            for attr in &mut field.attrs {
                if !attr.path().is_ident("serde") {
                    continue;
                }
                let mut attrs =
                    attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
                for meta in &mut attrs {
                    if meta.path().is_ident("flatten") {
                        flatten = true;
                        real_flatten = true;
                    }
                    if meta.path().is_ident("skip") || meta.path().is_ident("skip_serializing") {
                        skipped = true;
                    }
                    if ["with", "serialize_with"]
                        .iter()
                        .any(|x| meta.path().is_ident(x))
                    {
                        if adapter.is_none() {
                            return Err(syn::Error::new_spanned(
                                meta,
                                "custom field serialization requires #[action_input(adapter = Path)]",
                            ));
                        }
                    }
                    if let Meta::NameValue(nv) = meta {
                        if nv.path.is_ident("rename") {
                            if let syn::Expr::Lit(lit) = &nv.value {
                                if let syn::Lit::Str(s) = &lit.lit {
                                    wire = s.value();
                                }
                            }
                        }
                        if nv.path.is_ident("skip_serializing_if") {
                            conditional = true;
                            let syn::Expr::Lit(lit) = &nv.value else {
                                continue;
                            };
                            let syn::Lit::Str(s) = &lit.lit else { continue };
                            let predicate: syn::Path = s.parse()?;
                            let helper = format_ident!("__omochi_{}_skip_{}", name.unraw(), count);
                            count += 1;
                            predicates.push(quote! { #[allow(non_snake_case)] fn #helper #helper_generics (value: &#helper_ty) -> bool #helper_where { value.literal_matches(|v| #predicate(v)) } });
                            let helper_path = if helper_args.is_empty() {
                                helper.to_string()
                            } else {
                                quote!(#helper::<#(#helper_args),*>).to_string()
                            };
                            let path = syn::LitStr::new(&helper_path, s.span());
                            nv.value = parse_quote!(#path);
                        }
                    }
                }
                let attrs: Punctuated<Meta, Token![,]> = attrs
                    .into_iter()
                    .filter(|m| !m.path().is_ident("with") && !m.path().is_ident("serialize_with"))
                    .collect();
                *attr = parse_quote!(#[serde(#attrs)]);
            }
            if !skipped && (real_flatten || conditional) {
                let helper = format_ident!("__omochi_{}_serialize_{}", name.unraw(), count);
                count += 1;
                let mut hg = input.generics.clone();
                hg.params.push(parse_quote!(__S: ::serde::Serializer));
                if adapter.is_none() {
                    hg.make_where_clause()
                        .predicates
                        .push(parse_quote!(#ty: ::serde::Serialize));
                }
                let (ig, _, wc) = hg.split_for_impl();
                predicates.push(quote! {
                    #[allow(non_snake_case)] fn #helper #ig(value: &#helper_ty, serializer: __S) -> ::std::result::Result<__S::Ok, __S::Error> #wc {
                        ::omochi_ui::serialize_input_field(value, serializer, #real_flatten, #conditional)
                    }
                });
                let helper_path = if helper_args.is_empty() {
                    helper.to_string()
                } else {
                    quote!(#helper::<#(#helper_args,)* _>).to_string()
                };
                let path = syn::LitStr::new(&helper_path, name.span());
                field
                    .attrs
                    .push(parse_quote!(#[serde(serialize_with = #path)]));
            }
            if !skipped && adapter.is_none() {
                all_types.push(ty.clone());
            }
            field.attrs.retain(|a| a.path().is_ident("serde"));
            field.ty = if let Some(adapter) = &adapter {
                parse_quote!(::omochi_ui::AdaptedInput<#ty, #adapter>)
            } else {
                parse_quote!(::omochi_ui::Input<#ty>)
            };
            if field.ident.is_none() {
                wire = serialized_index.to_string();
                // A conditional field can disappear or shift every later tuple slot.
                if conditional && !skipped {
                    stable_position = false;
                }
            }
            if !skipped {
                serialized_index += 1;
            }
            if project && !skipped && stable_position {
                let method = format_ident!(
                    "field_{}",
                    field
                        .ident
                        .as_ref()
                        .map(|x| x.unraw().to_string())
                        .unwrap_or(index.to_string())
                );
                let path = if flatten {
                    String::new()
                } else {
                    format!("/{}", wire.replace('~', "~0").replace('/', "~1"))
                };
                let path = if let Some(adapter) = &adapter {
                    quote!(<#adapter as ::omochi_ui::ExpressionAdapter<#ty>>::project(#path))
                } else {
                    quote!(#path.to_owned())
                };
                let projected = if let Some(a) = &adapter {
                    quote!(<#a as ::omochi_ui::ExpressionAdapter<#ty>>::Wire)
                } else {
                    quote!(#ty)
                };
                let access = projection_access(real_flatten, flatten);
                projections.push(quote! { pub fn #method() -> ::omochi_ui::Field<Self, #projected, #access> { ::omochi_ui::Field::new(#path) } });
            }
        }
        Ok(())
    };
    match &mut mirror.data {
        Data::Struct(s) => transform(&mut s.fields, true, &rename_all, false, false)?,
        Data::Enum(e) => {
            let internally_tagged = serde_name(&input.attrs, "tag")?.is_some()
                && serde_name(&input.attrs, "content")?.is_none();
            for v in &mut e.variants {
                let rule = serde_name(&v.attrs, "rename_all")?
                    .or(serde_name(&input.attrs, "rename_all_fields")?)
                    .unwrap_or_default();
                let tagged = internally_tagged && !serde_flag(&v.attrs, "untagged")?;
                transform(
                    &mut v.fields,
                    false,
                    &rule,
                    tagged,
                    skip_serializing(&v.attrs)?,
                )?;
            }
        }
        Data::Union(_) => return Err(syn::Error::new_spanned(name, "unions are not JSON inputs")),
    }
    // Concrete serialization remains delegated to Serde. Generated fields carry the same bounds.
    for ty in &all_types {
        mirror
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: ::serde::Serialize));
    }
    let mut generics = input.generics.clone();
    let (_, original_types, _) = input.generics.split_for_impl();
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #original_types: ::serde::Serialize));
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#generated #original_types: ::serde::Serialize));
    for ty in &all_types {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: ::serde::Serialize));
    }
    let (impl_g, ty_g, where_g) = generics.split_for_impl();
    let mut variants = Vec::new();
    if let Data::Enum(data) = &input.data {
        let vis = &input.vis;
        let tag = serde_name(&input.attrs, "tag")?;
        let content = serde_name(&input.attrs, "content")?;
        let untagged = serde_flag(&input.attrs, "untagged")?;
        for variant in &data.variants {
            if skip_serializing(&variant.attrs)? {
                continue;
            }
            let id = &variant.ident;
            let marker = format_ident!("{}{}Fields", name.unraw(), id.unraw());
            let method = format_ident!("variant_{}", snake(&id.unraw().to_string()));
            let wire = serde_name(&variant.attrs, "rename")?
                .unwrap_or_else(|| rename_variant(&id.unraw().to_string(), &rename_all));
            let variant_untagged = untagged || serde_flag(&variant.attrs, "untagged")?;
            let internally_tagged = !variant_untagged && tag.is_some() && content.is_none();
            let path = if variant_untagged || internally_tagged {
                String::new()
            } else {
                format!(
                    "/{}",
                    content
                        .as_ref()
                        .unwrap_or(&wire)
                        .replace('~', "~0")
                        .replace('/', "~1")
                )
            };
            let pattern = match &variant.fields {
                Fields::Named(_) => quote!(Self::#id { .. }),
                Fields::Unnamed(_) => quote!(Self::#id(..)),
                Fields::Unit => quote!(Self::#id),
            };
            let mut methods = Vec::new();
            let newtype = matches!(&variant.fields, Fields::Unnamed(f) if f.unnamed.len() == 1);
            let fields_rule = serde_name(&variant.attrs, "rename_all")?
                .or(serde_name(&input.attrs, "rename_all_fields")?)
                .unwrap_or_default();
            let mut serialized_index = 0usize;
            let mut stable_position = true;
            for (index, field) in variant.fields.iter().enumerate() {
                let ty = &field.ty;
                let metas: Vec<Meta> = field
                    .attrs
                    .iter()
                    .filter(|a| a.path().is_ident("serde"))
                    .map(|a| a.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated))
                    .collect::<syn::Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .collect();
                if metas
                    .iter()
                    .any(|m| m.path().is_ident("skip") || m.path().is_ident("skip_serializing"))
                {
                    continue;
                }
                if field.ident.is_none()
                    && metas
                        .iter()
                        .any(|m| m.path().is_ident("skip_serializing_if"))
                {
                    stable_position = false;
                }
                if !stable_position {
                    continue;
                }
                let flatten = metas.iter().any(|m| m.path().is_ident("flatten"));
                let raw = field
                    .ident
                    .as_ref()
                    .map(|i| i.unraw().to_string())
                    .unwrap_or(index.to_string());
                let field_method = format_ident!("field_{}", raw);
                let wire = if field.ident.is_none() {
                    serialized_index.to_string()
                } else {
                    serde_name(&field.attrs, "rename")?
                        .unwrap_or_else(|| rename_field(&raw, &fields_rule))
                };
                serialized_index += 1;
                let path = if newtype || flatten {
                    String::new()
                } else {
                    format!("/{}", wire.replace('~', "~0").replace('/', "~1"))
                };
                let path = if let Some(a) = adapter(&field.attrs)? {
                    quote!(<#a as ::omochi_ui::ExpressionAdapter<#ty>>::project(#path))
                } else {
                    quote!(#path.to_owned())
                };
                let projected = if let Some(a) = adapter(&field.attrs)? {
                    quote!(<#a as ::omochi_ui::ExpressionAdapter<#ty>>::Wire)
                } else {
                    quote!(#ty)
                };
                let access = projection_access(flatten, newtype);
                methods.push(quote!(pub fn #field_method() -> ::omochi_ui::Field<Self, #projected, #access> { ::omochi_ui::Field::new(#path) }));
            }
            variants.push(quote! {
                #vis struct #marker #generics #where_g { marker: ::std::marker::PhantomData<fn() -> #name #ty_g> }
                impl #impl_g #marker #ty_g #where_g { #(#methods)* }
            });
            let access = projection_access(internally_tagged, variant_untagged);
            projections.push(quote! {
                pub fn #method() -> ::omochi_ui::Variant<Self, #marker #ty_g, #access> where Self: ::serde::de::DeserializeOwned {
                    ::omochi_ui::Variant::new(#path.into(), |value| {
                        match ::serde_json::from_value::<Self>(value.clone()) { Ok(v) => matches!(v, #pattern), Err(_) => false }
                    })
                }
            });
        }
    }
    Ok(quote! {
        #mirror
        #(#variants)*
        #(#predicates)*
        impl #impl_g #generated #ty_g #where_g {
            pub fn encode(self) -> ::omochi_ui::Result<::serde_json::Value> {
                <Self as ::omochi_ui::IntoActionInput<#name #ty_g>>::encode(self)
            }
        }
        impl #impl_g ::omochi_ui::IntoActionInput<#name #ty_g> for #generated #ty_g #where_g {
            fn encode(self) -> ::omochi_ui::Result<::serde_json::Value> {
                ::omochi_ui::checked_json(self)
            }
        }
        impl #impl_g ::omochi_ui::ActionInput for #name #ty_g #where_g { type Expression = #generated #ty_g; }
        impl #impl_g #name #ty_g #where_g { #(#projections)* }
    })
}

fn projection_access(flatten: bool, alias: bool) -> proc_macro2::TokenStream {
    if flatten {
        quote!(::omochi_ui::FieldsOnly)
    } else if alias {
        quote!(::omochi_ui::SameValue)
    } else {
        quote!(::omochi_ui::WholeValue)
    }
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}
fn rename_variant(name: &str, rule: &str) -> String {
    match rule {
        "snake_case" | "SCREAMING_SNAKE_CASE" | "kebab-case" | "SCREAMING-KEBAB-CASE" => {
            rename_field(&snake(name), rule)
        }
        "lowercase" => name.to_lowercase(),
        "UPPERCASE" => name.to_uppercase(),
        "camelCase" => {
            let mut s = name.to_owned();
            if !s.is_empty() {
                s.replace_range(..1, &s[..1].to_lowercase());
            }
            s
        }
        _ => name.into(),
    }
}
