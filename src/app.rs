use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};

use crate::components::layout::Layout;
use crate::pages::admin::{AdminGit, AdminLogin};
use crate::pages::archive::Archive;
use crate::pages::home::Home;
use crate::pages::not_found::NotFound;
use crate::pages::page_view::PageView;
use crate::pages::post_detail::PostDetail;
use crate::pages::post_list::PostList;
use crate::pages::search::Search;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Layout>
                <Routes fallback=NotFound>
                    <Route path=leptos_router::path!("/") view=Home/>
                    <Route path=leptos_router::path!("/posts") view=PostList/>
                    <Route path=leptos_router::path!("/posts/:slug") view=PostDetail/>
                    <Route path=leptos_router::path!("/archive") view=Archive/>
                    <Route path=leptos_router::path!("/search") view=Search/>
                    <Route path=leptos_router::path!("/pages/:slug") view=PageView/>
                    <Route path=leptos_router::path!("/admin/login") view=AdminLogin/>
                    <Route path=leptos_router::path!("/admin") view=AdminGit/>
                </Routes>
            </Layout>
        </Router>
    }
}
