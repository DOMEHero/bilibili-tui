use crate::api::{
    ApiClient,
    bangumi::{SeasonRankItem, SeasonResult},
    comment::CommentItem,
    dynamic::DynamicItem,
    dynamic::UpListItem,
    favorite::{
        CollectedFolder, FavoriteFolder, FavoriteOrder, FavoriteResourceData, FavoriteSource,
        SeasonArchivesData, WatchLaterData,
    },
    history::HistoryCursor,
    history::HistoryData,
    live::LiveRoom,
    recommend::VideoItem,
    search::HotwordItem,
    search::SearchVideoItem,
    space::{RelationStat, SpaceInfo, SpaceVideoData, SpaceVideoOrder},
    video::RelatedVideoItem,
    video::VideoInfo,
};
use crate::presentation::tui::DynamicTab;
use std::sync::{Arc, mpsc};

#[derive(Debug)]
pub enum NetworkCommand {
    LoadHome {
        req_id: u64,
        use_guest_feed: bool,
    },
    LoadHomeMore {
        req_id: u64,
        fresh_idx: i32,
        use_guest_feed: bool,
    },
    LoadHotwords {
        req_id: u64,
    },
    Search {
        req_id: u64,
        keyword: String,
        page: i32,
    },
    LoadDynamicInit {
        req_id: u64,
        tab: DynamicTab,
        host_mid: Option<i64>,
    },
    LoadDynamicRefresh {
        req_id: u64,
        tab: DynamicTab,
        host_mid: Option<i64>,
    },
    LoadDynamicMore {
        req_id: u64,
        offset: String,
        tab: DynamicTab,
        host_mid: Option<i64>,
    },
    LoadHistoryInit {
        req_id: u64,
    },
    LoadHistoryMore {
        req_id: u64,
        cursor: HistoryCursor,
    },
    LoadLiveInit {
        req_id: u64,
    },
    LoadLiveMore {
        req_id: u64,
    },
    LoadVideoDetail {
        req_id: u64,
        bvid: String,
        aid: i64,
    },
    LoadUpPage {
        req_id: u64,
        mid: i64,
        order: SpaceVideoOrder,
    },
    LoadUpVideos {
        req_id: u64,
        mid: i64,
        page: i32,
        order: SpaceVideoOrder,
    },
    LoadFavoriteResources {
        req_id: u64,
        owner_mid: i64,
        media_id: i64,
        page: i32,
        order: FavoriteOrder,
    },
    LoadFavoritesInit {
        req_id: u64,
        mid: i64,
    },
    LoadFavoritesContent {
        req_id: u64,
        source: FavoriteSource,
        page: i32,
    },
    LoadDynamicDetail {
        req_id: u64,
        dynamic_id: String,
    },
    LoadBangumiIndex {
        req_id: u64,
    },
    LoadBangumiDetail {
        req_id: u64,
        season_id: i64,
    },
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum NetworkEvent {
    HomeLoaded {
        req_id: u64,
        videos: Vec<VideoItem>,
    },
    HomeMoreLoaded {
        req_id: u64,
        videos: Vec<VideoItem>,
    },
    HotwordsLoaded {
        req_id: u64,
        hotwords: Vec<HotwordItem>,
    },
    SearchLoaded {
        req_id: u64,
        keyword: String,
        page: i32,
        results: Vec<SearchVideoItem>,
        total: i32,
    },
    DynamicLoaded {
        req_id: u64,
        append: bool,
        up_list: Option<Vec<UpListItem>>,
        items: Vec<crate::api::dynamic::DynamicItem>,
        offset: Option<String>,
        has_more: bool,
    },
    HistoryLoaded {
        req_id: u64,
        append: bool,
        data: HistoryData,
    },
    LiveLoaded {
        req_id: u64,
        append: bool,
        rooms: Vec<LiveRoom>,
    },
    VideoDetailLoaded {
        req_id: u64,
        bvid: String,
        video_info: VideoInfo,
        comments: Vec<CommentItem>,
        has_more_comments: bool,
        related_videos: Vec<RelatedVideoItem>,
    },
    UpPageLoaded {
        req_id: u64,
        mid: i64,
        order: SpaceVideoOrder,
        profile: SpaceInfo,
        relation: Option<RelationStat>,
        videos: SpaceVideoData,
        folders: Vec<FavoriteFolder>,
    },
    UpVideosLoaded {
        req_id: u64,
        mid: i64,
        page: i32,
        order: SpaceVideoOrder,
        videos: SpaceVideoData,
    },
    FavoriteResourcesLoaded {
        req_id: u64,
        owner_mid: i64,
        media_id: i64,
        page: i32,
        order: FavoriteOrder,
        resources: FavoriteResourceData,
    },
    FavoritesInitLoaded {
        req_id: u64,
        mid: i64,
        watch_later: WatchLaterData,
        created: Vec<FavoriteFolder>,
        collected: Vec<CollectedFolder>,
    },
    FavoritesWatchLaterLoaded {
        req_id: u64,
        page: i32,
        data: WatchLaterData,
    },
    FavoritesCreatedLoaded {
        req_id: u64,
        media_id: i64,
        page: i32,
        data: FavoriteResourceData,
    },
    FavoritesCollectedLoaded {
        req_id: u64,
        season_id: i64,
        page: i32,
        data: SeasonArchivesData,
    },
    DynamicDetailLoaded {
        req_id: u64,
        dynamic_id: String,
        dynamic_item: DynamicItem,
        comments: Vec<CommentItem>,
        has_more_comments: bool,
        image_urls: Vec<String>,
    },
    BangumiIndexLoaded {
        req_id: u64,
        items: Vec<SeasonRankItem>,
    },
    BangumiDetailLoaded {
        req_id: u64,
        season_id: i64,
        season: SeasonResult,
    },
    RequestFailed {
        req_id: u64,
        target: &'static str,
        error: String,
    },
}

pub struct NetworkBridge {
    pub command_tx: mpsc::Sender<NetworkCommand>,
    pub event_rx: mpsc::Receiver<NetworkEvent>,
}

pub fn start_network_worker(api_client: Arc<ApiClient>) -> NetworkBridge {
    let (command_tx, command_rx) = mpsc::channel::<NetworkCommand>();
    let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>();

    std::thread::Builder::new()
        .name("bilibili-network-worker".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return,
            };

            while let Ok(command) = command_rx.recv() {
                let event = runtime.block_on(handle_command(api_client.clone(), command));
                if event_tx.send(event).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn network worker");

    NetworkBridge {
        command_tx,
        event_rx,
    }
}

async fn handle_command(api_client: Arc<ApiClient>, command: NetworkCommand) -> NetworkEvent {
    match command {
        NetworkCommand::LoadHome {
            req_id,
            use_guest_feed,
        } => match if use_guest_feed {
            api_client.get_popular_videos(1, 20).await
        } else {
            api_client.get_recommendations().await
        } {
            Ok(videos) => NetworkEvent::HomeLoaded { req_id, videos },
            Err(e) => failed(req_id, "home", e),
        },
        NetworkCommand::LoadHomeMore {
            req_id,
            fresh_idx,
            use_guest_feed,
        } => {
            match if use_guest_feed {
                api_client.get_popular_videos(fresh_idx, 20).await
            } else {
                api_client.get_recommendations_paged(fresh_idx).await
            } {
                Ok(videos) => NetworkEvent::HomeMoreLoaded { req_id, videos },
                Err(e) => failed(req_id, "home_more", e),
            }
        }
        NetworkCommand::LoadHotwords { req_id } => match api_client.get_hot_search().await {
            Ok(hotwords) => NetworkEvent::HotwordsLoaded { req_id, hotwords },
            Err(e) => failed(req_id, "hotwords", e),
        },
        NetworkCommand::Search {
            req_id,
            keyword,
            page,
        } => match api_client.search_videos(&keyword, page).await {
            Ok(data) => NetworkEvent::SearchLoaded {
                req_id,
                keyword,
                page,
                results: data.result.unwrap_or_default(),
                total: data.num_results.unwrap_or(0),
            },
            Err(e) => failed(req_id, "search", e),
        },
        NetworkCommand::LoadUpPage { req_id, mid, order } => {
            match api_client.get_space_info(mid).await {
                Ok(profile) => {
                    let relation = api_client.get_relation_stat(mid).await.ok();
                    let folders = api_client
                        .get_favorite_folders(mid)
                        .await
                        .unwrap_or_default();
                    match api_client.get_space_videos(mid, 1, 40, order).await {
                        Ok(videos) => NetworkEvent::UpPageLoaded {
                            req_id,
                            mid,
                            order,
                            profile,
                            relation,
                            videos,
                            folders,
                        },
                        Err(error) => failed(req_id, "up_page", error),
                    }
                }
                Err(error) => failed(req_id, "up_page", error),
            }
        }
        NetworkCommand::LoadUpVideos {
            req_id,
            mid,
            page,
            order,
        } => match api_client.get_space_videos(mid, page, 40, order).await {
            Ok(videos) => NetworkEvent::UpVideosLoaded {
                req_id,
                mid,
                page,
                order,
                videos,
            },
            Err(error) => failed(req_id, "up_videos", error),
        },
        NetworkCommand::LoadFavoriteResources {
            req_id,
            owner_mid,
            media_id,
            page,
            order,
        } => match api_client
            .get_favorite_resources(media_id, page, 40, order)
            .await
        {
            Ok(resources) => NetworkEvent::FavoriteResourcesLoaded {
                req_id,
                owner_mid,
                media_id,
                page,
                order,
                resources,
            },
            Err(error) => failed(req_id, "favorite_resources", error),
        },
        NetworkCommand::LoadFavoritesInit { req_id, mid } => {
            match api_client.get_watch_later(1, 20).await {
                Ok(watch_later) => {
                    let created = api_client
                        .get_favorite_folders(mid)
                        .await
                        .unwrap_or_default();
                    match api_client.get_collected_folders(mid, 1, 50).await {
                        Ok(collected) => NetworkEvent::FavoritesInitLoaded {
                            req_id,
                            mid,
                            watch_later,
                            created,
                            collected: collected.list,
                        },
                        Err(error) => failed(req_id, "favorites_init", error),
                    }
                }
                Err(error) => failed(req_id, "favorites_init", error),
            }
        }
        NetworkCommand::LoadFavoritesContent {
            req_id,
            source,
            page,
        } => match source {
            FavoriteSource::WatchLater => match api_client.get_watch_later(page, 20).await {
                Ok(data) => NetworkEvent::FavoritesWatchLaterLoaded { req_id, page, data },
                Err(error) => failed(req_id, "favorites_content", error),
            },
            FavoriteSource::Created { media_id, .. } => match api_client
                .get_favorite_resources(media_id, page, 40, FavoriteOrder::RecentlyFavorited)
                .await
            {
                Ok(data) => NetworkEvent::FavoritesCreatedLoaded {
                    req_id,
                    media_id,
                    page,
                    data,
                },
                Err(error) => failed(req_id, "favorites_content", error),
            },
            FavoriteSource::Collected { season_id, mid, .. } => match api_client
                .get_collected_season_videos(mid, season_id, page, 30)
                .await
            {
                Ok(data) => NetworkEvent::FavoritesCollectedLoaded {
                    req_id,
                    season_id,
                    page,
                    data,
                },
                Err(error) => failed(req_id, "favorites_content", error),
            },
        },
        NetworkCommand::LoadDynamicInit {
            req_id,
            tab,
            host_mid,
        } => {
            let up_list = match api_client.get_dynamic_portal().await {
                Ok(portal) => portal.up_list,
                Err(_) => None,
            };
            let feed_type = tab.get_feed_type();
            match api_client.get_dynamic_feed(None, feed_type, host_mid).await {
                Ok(data) => NetworkEvent::DynamicLoaded {
                    req_id,
                    append: false,
                    up_list,
                    items: data.items.unwrap_or_default(),
                    offset: data.offset,
                    has_more: data.has_more.unwrap_or(false),
                },
                Err(e) => failed(req_id, "dynamic_init", e),
            }
        }
        NetworkCommand::LoadDynamicRefresh {
            req_id,
            tab,
            host_mid,
        } => {
            let feed_type = tab.get_feed_type();
            match api_client.get_dynamic_feed(None, feed_type, host_mid).await {
                Ok(data) => NetworkEvent::DynamicLoaded {
                    req_id,
                    append: false,
                    up_list: None,
                    items: data.items.unwrap_or_default(),
                    offset: data.offset,
                    has_more: data.has_more.unwrap_or(false),
                },
                Err(e) => failed(req_id, "dynamic_refresh", e),
            }
        }
        NetworkCommand::LoadDynamicMore {
            req_id,
            offset,
            tab,
            host_mid,
        } => {
            let feed_type = tab.get_feed_type();
            match api_client
                .get_dynamic_feed(Some(&offset), feed_type, host_mid)
                .await
            {
                Ok(data) => NetworkEvent::DynamicLoaded {
                    req_id,
                    append: true,
                    up_list: None,
                    items: data.items.unwrap_or_default(),
                    offset: data.offset,
                    has_more: data.has_more.unwrap_or(false),
                },
                Err(e) => failed(req_id, "dynamic_more", e),
            }
        }
        NetworkCommand::LoadHistoryInit { req_id } => {
            match api_client.get_history(None, None, None).await {
                Ok(data) => NetworkEvent::HistoryLoaded {
                    req_id,
                    append: false,
                    data,
                },
                Err(e) => failed(req_id, "history_init", e),
            }
        }
        NetworkCommand::LoadHistoryMore { req_id, cursor } => match api_client
            .get_history(
                Some(cursor.max),
                Some(cursor.view_at),
                Some(cursor.business.as_str()),
            )
            .await
        {
            Ok(data) => NetworkEvent::HistoryLoaded {
                req_id,
                append: true,
                data,
            },
            Err(e) => failed(req_id, "history_more", e),
        },
        NetworkCommand::LoadLiveInit { req_id } => {
            let rooms = match api_client.get_live_home_rooms().await {
                Ok(rooms) => Ok(rooms),
                Err(_) => api_client.get_live_recommendations().await,
            };
            match rooms {
                Ok(rooms) => NetworkEvent::LiveLoaded {
                    req_id,
                    append: false,
                    rooms,
                },
                Err(e) => failed(req_id, "live_init", e),
            }
        }
        NetworkCommand::LoadLiveMore { req_id } => {
            match api_client.get_live_recommendations().await {
                Ok(rooms) => NetworkEvent::LiveLoaded {
                    req_id,
                    append: true,
                    rooms,
                },
                Err(e) => failed(req_id, "live_more", e),
            }
        }
        NetworkCommand::LoadVideoDetail { req_id, bvid, aid } => {
            let video_info = match api_client.get_video_info(&bvid).await {
                Ok(info) => info,
                Err(e) => return failed(req_id, "video_detail", e),
            };
            let (comments, has_more_comments) = match api_client.get_comments(aid, 1).await {
                Ok(data) => {
                    let comments = data.replies.unwrap_or_default();
                    let has_more = data
                        .page
                        .map(|p| p.count.unwrap_or(0) > comments.len() as i32)
                        .unwrap_or(false);
                    (comments, has_more)
                }
                Err(_) => (Vec::new(), false),
            };
            let related_videos = api_client
                .get_related_videos(&bvid)
                .await
                .unwrap_or_default();
            NetworkEvent::VideoDetailLoaded {
                req_id,
                bvid,
                video_info,
                comments,
                has_more_comments,
                related_videos,
            }
        }
        NetworkCommand::LoadDynamicDetail { req_id, dynamic_id } => {
            let dynamic_item = match api_client.get_dynamic_detail(&dynamic_id).await {
                Ok(item) => item,
                Err(e) => return failed(req_id, "dynamic_detail", e),
            };
            let comment_type = dynamic_item.comment_type();
            let comment_oid = dynamic_item.comment_oid(&dynamic_id);
            let (comments, has_more_comments) = if let Some(oid) = comment_oid {
                match api_client.get_dynamic_comments(oid, comment_type, 1).await {
                    Ok(data) => {
                        let comments = data.replies.unwrap_or_default();
                        let has_more = data
                            .page
                            .map(|p| p.count.unwrap_or(0) > comments.len() as i32)
                            .unwrap_or(false);
                        (comments, has_more)
                    }
                    Err(_) => (Vec::new(), false),
                }
            } else {
                (Vec::new(), false)
            };
            let mut image_urls = Vec::new();
            if dynamic_item.is_draw() {
                image_urls.extend(
                    dynamic_item
                        .draw_images()
                        .into_iter()
                        .map(|s| s.to_string()),
                );
            }
            if dynamic_item.is_opus() {
                image_urls.extend(
                    dynamic_item
                        .opus_images()
                        .into_iter()
                        .map(|s| s.to_string()),
                );
            }
            NetworkEvent::DynamicDetailLoaded {
                req_id,
                dynamic_id,
                dynamic_item,
                comments,
                has_more_comments,
                image_urls,
            }
        }
        NetworkCommand::LoadBangumiIndex { req_id } => match api_client.get_bangumi_rank().await {
            Ok(items) => NetworkEvent::BangumiIndexLoaded { req_id, items },
            Err(e) => failed(req_id, "bangumi_index", e),
        },
        NetworkCommand::LoadBangumiDetail { req_id, season_id } => {
            match api_client.get_bangumi_season(season_id).await {
                Ok(season) => NetworkEvent::BangumiDetailLoaded {
                    req_id,
                    season_id,
                    season,
                },
                Err(e) => failed(req_id, "bangumi_detail", e),
            }
        }
    }
}

fn failed(req_id: u64, target: &'static str, error: anyhow::Error) -> NetworkEvent {
    if let Some(mut dir) = dirs::config_dir() {
        dir.push("bilibili-tui");
        if std::fs::create_dir_all(&dir).is_ok()
            && let Ok(mut log) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("debug.log"))
        {
            use std::io::Write;
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(
                log,
                "[{timestamp}] Network request failed\nTarget: {target}\nRequest ID: {req_id}\nError: {error:#}\n"
            );
        }
    }

    NetworkEvent::RequestFailed {
        req_id,
        target,
        error: error.to_string(),
    }
}
