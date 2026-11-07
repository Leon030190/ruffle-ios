use std::cell::OnceCell;

use block2::{Block, RcBlock};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass as _, Message};
use objc2_core_data::{
    NSFetchedResultsChangeType, NSFetchedResultsController, NSFetchedResultsControllerDelegate,
    NSFetchedResultsSectionInfo,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSBundle, NSCoder, NSIndexPath, NSInteger, NSObject,
    NSObjectProtocol, NSString, NSURL,
};
use objc2_ui_kit::{
    NSDataAsset, UIBarButtonItem, UIDocumentPickerDelegate, UIDocumentPickerViewController,
    UILabel, UITableView, UITableViewCell, UITableViewCellEditingStyle, UITableViewController,
    UITableViewDataSource, UITableViewRowAnimation,
};
#[allow(deprecated)]
use objc2_ui_kit::{UIDocumentPickerMode, UIStoryboardSegue};
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::PlayerBuilder;
use ruffle_frontend_utils::backends::audio::CpalAudioBackend;

use crate::edit_controller::EditController;
use crate::storage::Movie;
use crate::{storage, PlayerController, PlayerView};

// There is no standardized UTI for SWFs, so this is one we picked.
pub const SWF_UTI: &str = "com.adobe.swf";

// Temporary until we publish the package
pub const RUF_UTI: &str = "com.example.rs.ruffle.bundle";

#[derive(Debug)]
pub struct Ivars {
    logo_view: OnceCell<Retained<PlayerView>>,
    fetched_movies: Retained<NSFetchedResultsController<Movie>>,
}

define_class!(
    #[unsafe(super(UITableViewController))]
    #[name = "LibraryController"]
    #[ivars = Ivars]
    #[derive(Debug)]
    pub struct LibraryController;

    unsafe impl NSObjectProtocol for LibraryController {}

    /// UIViewController.
    impl LibraryController {
        #[unsafe(method_id(initWithNibName:bundle:))]
        fn _init_with_nib_name_bundle(
            this: Allocated<Self>,
            nib_name_or_nil: Option<&NSString>,
            nib_bundle_or_nil: Option<&NSBundle>,
        ) -> Retained<Self> {
            tracing::info!("library init");
            let this = this.set_ivars(Ivars {
                logo_view: Default::default(),
                fetched_movies: storage::all_movies(),
            });
            let this: Retained<Self> = unsafe {
                msg_send![super(this), initWithNibName: nib_name_or_nil, bundle: nib_bundle_or_nil]
            };
            unsafe {
                this.ivars()
                    .fetched_movies
                    .setDelegate(Some(ProtocolObject::from_ref(&*this)))
            };
            this
        }

        #[unsafe(method_id(initWithCoder:))]
        fn _init_with_coder(this: Allocated<Self>, coder: &NSCoder) -> Option<Retained<Self>> {
            tracing::info!("library init");
            let this = this.set_ivars(Ivars {
                logo_view: Default::default(),
                fetched_movies: storage::all_movies(),
            });
            let this: Option<Retained<Self>> =
                unsafe { msg_send![super(this), initWithCoder: coder] };
            if let Some(this) = &this {
                unsafe {
                    this.ivars()
                        .fetched_movies
                        .setDelegate(Some(ProtocolObject::from_ref(&**this)));
                }
            }
            this
        }

        #[unsafe(method(viewDidLoad))]
        fn _view_did_load(&self) {
            // Xcode template calls super at the beginning
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };
            self.view_did_load();
        }

        #[unsafe(method(viewIsAppearing:))]
        fn _view_is_appearing(&self, animated: bool) {
            self.view_is_appearing(animated);
            // Docs say to call super
            let _: () = unsafe { msg_send![super(self), viewIsAppearing: animated] };
        }

        #[unsafe(method(viewWillDisappear:))]
        fn _view_will_disappear(&self, animated: bool) {
            self.view_will_disappear(animated);
            // Docs say to call super
            let _: () = unsafe { msg_send![super(self), viewWillDisappear: animated] };
        }

        #[unsafe(method(viewDidDisappear:))]
        fn _view_did_disappear(&self, animated: bool) {
            self.view_did_disappear(animated);
            // Docs say to call super
            let _: () = unsafe { msg_send![super(self), viewDidDisappear: animated] };
        }

        #[unsafe(method(prepareForSegue:sender:))]
        #[allow(deprecated)]
        fn _prepare_for_segue(&self, segue: &UIStoryboardSegue, sender: Option<&NSObject>) {
            self.prepare_for_segue(segue, sender.expect("has sender"));
        }
    }

    /// Storyboard
    /// See storyboard_connections.h
    impl LibraryController {
        #[unsafe(method(setLogoView:))]
        fn _set_logo_view(&self, view: Option<&AnyObject>) {
            tracing::trace!("library set logo view");
            let view = view
                .expect("logo view not null")
                .downcast_ref::<PlayerView>()
                .expect("logo view not a PlayerView");
            self.ivars()
                .logo_view
                .set(view.retain())
                .expect("only set logo view once");
        }

        #[unsafe(method(toggleEditing:))]
        fn _toggle_editing(&self, button: Option<&AnyObject>) {
            tracing::trace!("library toggle editing");
            let button = button
                .expect("edit button not null")
                .downcast_ref::<UIBarButtonItem>()
                .expect("edit button not UIBarButtonItem");
            self.toggle_editing(button);
        }

        #[unsafe(method(cancelEditItem:))]
        #[allow(deprecated)]
        fn _cancel_edit_item(&self, _segue: &UIStoryboardSegue) {}

        #[unsafe(method(saveEditItem:))]
        #[allow(deprecated)]
        fn _save_edit_item(&self, segue: &UIStoryboardSegue) {
            self.save_item(segue);
        }

        #[unsafe(method(showDocumentPicker:))]
        #[allow(deprecated)]
        fn _show_document_picker(&self, _sender: Option<&AnyObject>) {
            self.show_add_menu();
        }
    }

    #[allow(non_snake_case)]
    unsafe impl UITableViewDataSource for LibraryController {
        #[unsafe(method(tableView:numberOfRowsInSection:))]
        fn tableView_numberOfRowsInSection(
            &self,
            _table_view: &UITableView,
            section: NSInteger,
        ) -> NSInteger {
            let sections = unsafe { self.ivars().fetched_movies.sections().unwrap() };
            let section_info = sections.objectAtIndex(section as usize);
            unsafe { section_info.numberOfObjects() as isize }
        }

        #[unsafe(method(numberOfSectionsInTableView:))]
        fn numberOfSectionsInTableView(&self, _table_view: &UITableView) -> NSInteger {
            unsafe { self.ivars().fetched_movies.sections().unwrap().count() as NSInteger }
        }

        #[unsafe(method_id(tableView:cellForRowAtIndexPath:))]
        fn tableView_cellForRowAtIndexPath(
            &self,
            table_view: &UITableView,
            index_path: &NSIndexPath,
        ) -> Retained<UITableViewCell> {
            let cell = table_view.dequeueReusableCellWithIdentifier_forIndexPath(
                ns_string!("library-item"),
                index_path,
            );
            self.configure_cell(&cell, index_path);
            cell
        }

        #[unsafe(method_id(tableView:titleForHeaderInSection:))]
        fn tableView_titleForHeaderInSection(
            &self,
            _table_view: &UITableView,
            _section: NSInteger,
        ) -> Option<Retained<NSString>> {
            Some(NSString::from_str("Library"))
        }

        #[unsafe(method(tableView:commitEditingStyle:forRowAtIndexPath:))]
        fn tableView_commitEditingStyle_forRowAtIndexPath(
            &self,
            _table_view: &UITableView,
            editing_style: UITableViewCellEditingStyle,
            index_path: &NSIndexPath,
        ) {
            if editing_style == UITableViewCellEditingStyle::Delete {
                let movie = unsafe { self.ivars().fetched_movies.objectAtIndexPath(&index_path) };
                storage::delete_movie(&movie);
            }
        }

        // TODO: Implement moving (requires keeping the order in CoreData).
    }

    // For usage, see:
    // https://developer.apple.com/library/archive/samplecode/CoreDataBooks/Listings/Classes_RootViewController_m.html
    #[allow(non_snake_case)]
    unsafe impl NSFetchedResultsControllerDelegate for LibraryController {
        #[unsafe(method(controllerWillChangeContent:))]
        fn controllerWillChangeContent(&self, _controller: &NSFetchedResultsController) {
            self.tableView().unwrap().beginUpdates();
        }

        #[unsafe(method(controller:didChangeObject:atIndexPath:forChangeType:newIndexPath:))]
        fn controller_didChangeObject_atIndexPath_forChangeType_newIndexPath(
            &self,
            _controller: &NSFetchedResultsController,
            _an_object: &AnyObject,
            index_path: Option<&NSIndexPath>,
            r#type: NSFetchedResultsChangeType,
            new_index_path: Option<&NSIndexPath>,
        ) {
            let table_view = self.tableView().unwrap();

            match r#type {
                NSFetchedResultsChangeType::Insert => table_view
                    .insertRowsAtIndexPaths_withRowAnimation(
                        &NSArray::from_slice(&[new_index_path.unwrap()]),
                        UITableViewRowAnimation::Automatic,
                    ),
                NSFetchedResultsChangeType::Delete => table_view
                    .deleteRowsAtIndexPaths_withRowAnimation(
                        &NSArray::from_slice(&[index_path.unwrap()]),
                        UITableViewRowAnimation::Automatic,
                    ),
                NSFetchedResultsChangeType::Update => self.configure_cell(
                    &table_view
                        .cellForRowAtIndexPath(index_path.unwrap())
                        .unwrap(),
                    index_path.unwrap(),
                ),
                NSFetchedResultsChangeType::Move => {
                    table_view.deleteRowsAtIndexPaths_withRowAnimation(
                        &NSArray::from_slice(&[index_path.unwrap()]),
                        UITableViewRowAnimation::Automatic,
                    );
                    table_view.insertRowsAtIndexPaths_withRowAnimation(
                        &NSArray::from_slice(&[new_index_path.unwrap()]),
                        UITableViewRowAnimation::Automatic,
                    );
                }
                _ => {}
            }
        }

        #[unsafe(method(controllerDidChangeContent:))]
        fn controllerDidChangeContent(&self, _controller: &NSFetchedResultsController) {
            self.tableView().unwrap().endUpdates();
        }
    }

    #[allow(non_snake_case)]
    unsafe impl UIDocumentPickerDelegate for LibraryController {
        #[unsafe(method(documentPickerWasCancelled:))]
        fn documentPickerWasCancelled(&self, _controller: &UIDocumentPickerViewController) {
            tracing::info!("cancelled document picker");
        }

        #[unsafe(method(documentPicker:didPickDocumentAtURL:))]
        fn documentPicker_didPickDocumentAtURL(
            &self,
            _controller: &UIDocumentPickerViewController,
            url: &NSURL,
        ) {
            tracing::info!("completed document picker: {url:?}");
            if storage::movie_from_url(&url).is_none() {
                storage::add_movie(&url);
            } else {
                // TODO: Give the user an option here?
                tracing::error!("did not add existing movie {url:?}");
            }
        }
    }
);

impl LibraryController {
    fn logo_view(&self) -> &PlayerView {
        self.ivars().logo_view.get().expect("logo view initialized")
    }

    fn view_did_load(&self) {
        tracing::info!("library viewDidLoad");

        self.setup_logo();

        unsafe {
            self.ivars()
                .fetched_movies
                .performFetch()
                .expect("failed fetching movies")
        };
    }

    fn setup_logo(&self) {
        let view = self.logo_view();
        let asset = NSDataAsset::initWithName(NSDataAsset::alloc(), ns_string!("logo-anim"))
            .expect("asset store should contain logo-anim");
        let data = unsafe { asset.data() };
        // SAFETY: SwfMovie::from_data won't modify the NSData.
        let bytes = unsafe { data.as_bytes_unchecked() };
        let movie =
            SwfMovie::from_data(bytes, "file://logo-anim.swf".into(), None).expect("loading movie");

        let renderer = view.create_renderer();

        let mut builder = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_movie(movie);

        match CpalAudioBackend::new(None) {
            Ok(audio) => builder = builder.with_audio(audio),
            Err(e) => tracing::error!("Unable to create audio device: {e}"),
        }

        view.set_player(builder.build());
        // HACK: Skip first frame to avoid a flicker on startup
        // FIXME: This probably indicates a bug in our timing code?
        view.player_lock().run_frame();
    }

    fn view_is_appearing(&self, _animated: bool) {
        tracing::info!("library viewIsAppearing:");

        self.logo_view().start();
    }

    fn view_will_disappear(&self, _animated: bool) {
        tracing::info!("library viewWillDisappear:");

        self.logo_view().stop();
    }

    fn view_did_disappear(&self, _animated: bool) {
        tracing::info!("library viewDidDisappear:");

        self.logo_view().player_lock().flush_shared_objects();
    }

    #[allow(deprecated)]
    fn prepare_for_segue(&self, segue: &UIStoryboardSegue, sender: &NSObject) {
        let destination = segue.destinationViewController();
        tracing::info!(?destination, "prepareForSegue");

        // Identifiers are set up in the Storyboard
        let identifier = segue.identifier().expect("segue to have identifier");
        if &*identifier == ns_string!("add-item") {
            // No need to configure AddController
        } else if &*identifier == ns_string!("edit-item") {
            let edit_controller = destination.downcast_ref::<EditController>().unwrap();
            let cell = sender.downcast_ref::<UITableViewCell>().unwrap();

            let index_path = self.tableView().unwrap().indexPathForCell(&cell).unwrap();
            let movie = unsafe { self.ivars().fetched_movies.objectAtIndexPath(&index_path) };
            edit_controller.setup_movie(&movie);
        } else if &*identifier == ns_string!("run-item") {
            let player_controller = destination.downcast_ref::<PlayerController>().unwrap();
            let cell = sender.downcast_ref::<UITableViewCell>().unwrap();

            let index_path = self.tableView().unwrap().indexPathForCell(&cell).unwrap();
            let movie = unsafe { self.ivars().fetched_movies.objectAtIndexPath(&index_path) };
            player_controller.setup_movie(&movie);
        } else {
            unreachable!("unknown identifier for segue: {identifier:?}")
        }
    }

    #[allow(deprecated)]
    fn save_item(&self, segue: &UIStoryboardSegue) {
        tracing::info!("saveEditItem");
        let edit_controller = segue.sourceViewController();
        let edit_controller = edit_controller.downcast_ref::<EditController>().unwrap();
        dbg!(edit_controller); // TODO
    }

    /// Action Sheet menu offering multiple options to add Flash movies
    pub fn show_add_menu(&self) {
        let alert_cls = AnyClass::get(c"UIAlertController").expect("UIAlertController");
        let action_cls = AnyClass::get(c"UIAlertAction").expect("UIAlertAction");

        let alert: *mut AnyObject = unsafe {
            msg_send![
                alert_cls,
                alertControllerWithTitle: ns_string!("Thêm Flash Movie / Game"),
                message: ns_string!("Chọn phương thức thêm file SWF hoặc Ruffle bundle:"),
                preferredStyle: 0isize // 0 = ActionSheet
            ]
        };

        // Handle iPad popover anchor
        unsafe {
            let popover: *mut AnyObject = msg_send![alert, popoverPresentationController];
            if !popover.is_null() {
                if let Some(right_btn) = self.navigationItem().rightBarButtonItem() {
                    let _: () = msg_send![popover, setBarButtonItem: &right_btn];
                } else if let Some(view) = self.view() {
                    let _: () = msg_send![popover, setSourceView: &view];
                }
            }
        }

        // 1. Browse Files / iCloud
        let this = self.retain();
        let files_block = RcBlock::new(move |_action: *mut AnyObject| {
            this.show_document_picker();
        });
        let files_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("📂 Mở từ Tệp / iCloud Drive..."),
                style: 0isize,
                handler: &*files_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: files_action]; }

        // 2. Download from Web URL
        let this = self.retain();
        let download_block = RcBlock::new(move |_action: *mut AnyObject| {
            this.show_url_prompt(false);
        });
        let download_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("🌐 Tải từ URL Web (Download SWF)..."),
                style: 0isize,
                handler: &*download_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: download_action]; }

        // 3. Direct Online Stream URL
        let this = self.retain();
        let stream_block = RcBlock::new(move |_action: *mut AnyObject| {
            this.show_url_prompt(true);
        });
        let stream_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("🔗 Thêm link Web trực tuyến (Stream)..."),
                style: 0isize,
                handler: &*stream_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: stream_action]; }

        // 4. Sample Flash Demos & Games
        let this = self.retain();
        let sample_block = RcBlock::new(move |_action: *mut AnyObject| {
            this.show_sample_demos_menu();
        });
        let sample_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("🎮 Thêm Game Flash mẫu (Sample Demos)..."),
                style: 0isize,
                handler: &*sample_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: sample_action]; }

        // Cancel
        let cancel_block = RcBlock::new(|_action: *mut AnyObject| {});
        let cancel_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("Hủy"),
                style: 1isize,
                handler: &*cancel_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: cancel_action]; }

        unsafe {
            let alert_vc: Retained<AnyObject> = Retained::retain(alert).unwrap();
            let _: () = msg_send![self, presentViewController: &*alert_vc, animated: true, completion: None::<&Block<dyn Fn()>>];
        }
    }

    /// Prompt user with a text input alert to enter/paste a web URL
    fn show_url_prompt(&self, is_stream: bool) {
        let alert_cls = AnyClass::get(c"UIAlertController").expect("UIAlertController");
        let action_cls = AnyClass::get(c"UIAlertAction").expect("UIAlertAction");

        let title = if is_stream {
            ns_string!("Thêm Link Web Trực Tuyến")
        } else {
            ns_string!("Tải Flash SWF Từ Web")
        };
        let message = if is_stream {
            ns_string!("Nhập link SWF trực tiếp (Ruffle sẽ stream trực tiếp qua mạng):")
        } else {
            ns_string!("Nhập link URL của file .swf để tải về máy:")
        };

        let alert: *mut AnyObject = unsafe {
            msg_send![
                alert_cls,
                alertControllerWithTitle: title,
                message: message,
                preferredStyle: 1isize // 1 = Alert
            ]
        };

        // Text field
        let tf_block = RcBlock::new(|tf: *mut AnyObject| unsafe {
            let _: () = msg_send![tf, setPlaceholder: ns_string!("https://example.com/game.swf")];
            let _: () = msg_send![tf, setClearButtonMode: 1isize];
        });
        unsafe {
            let _: () = msg_send![alert, addTextFieldWithConfigurationHandler: &*tf_block];
        }

        // Confirm
        let alert_ptr = alert as usize;
        let confirm_block = RcBlock::new(move |_action: *mut AnyObject| {
            let alert_obj = alert_ptr as *mut AnyObject;
            let text_fields: *mut AnyObject = unsafe { msg_send![alert_obj, textFields] };
            if !text_fields.is_null() {
                let first_tf: *mut AnyObject = unsafe { msg_send![text_fields, firstObject] };
                if !first_tf.is_null() {
                    let text: *mut NSString = unsafe { msg_send![first_tf, text] };
                    if !text.is_null() {
                        let text_str = unsafe { (*text).to_string() };
                        let trimmed = text_str.trim();
                        if !trimmed.is_empty() {
                            if is_stream {
                                let _ = crate::web_downloader::add_direct_stream_url(trimmed);
                            } else {
                                let _ = crate::web_downloader::download_swf_from_url(trimmed);
                            }
                        }
                    }
                }
            }
        });

        let confirm_title = if is_stream { ns_string!("Thêm Stream") } else { ns_string!("Tải về") };
        let confirm_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: confirm_title,
                style: 0isize,
                handler: &*confirm_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: confirm_action]; }

        // Cancel
        let cancel_block = RcBlock::new(|_action: *mut AnyObject| {});
        let cancel_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("Hủy"),
                style: 1isize,
                handler: &*cancel_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: cancel_action]; }

        unsafe {
            let alert_vc: Retained<AnyObject> = Retained::retain(alert).unwrap();
            let _: () = msg_send![self, presentViewController: &*alert_vc, animated: true, completion: None::<&Block<dyn Fn()>>];
        }
    }

    /// Submenu for curated sample games and demos
    fn show_sample_demos_menu(&self) {
        let alert_cls = AnyClass::get(c"UIAlertController").expect("UIAlertController");
        let action_cls = AnyClass::get(c"UIAlertAction").expect("UIAlertAction");

        let alert: *mut AnyObject = unsafe {
            msg_send![
                alert_cls,
                alertControllerWithTitle: ns_string!("Chọn Game / Demo Flash"),
                message: ns_string!("Chọn tựa game hoặc demo mẫu để tự động tải về thư viện:"),
                preferredStyle: 0isize // ActionSheet
            ]
        };

        // Handle iPad popover anchor
        unsafe {
            let popover: *mut AnyObject = msg_send![alert, popoverPresentationController];
            if !popover.is_null() {
                if let Some(right_btn) = self.navigationItem().rightBarButtonItem() {
                    let _: () = msg_send![popover, setBarButtonItem: &right_btn];
                } else if let Some(view) = self.view() {
                    let _: () = msg_send![popover, setSourceView: &view];
                }
            }
        }

        for (idx, (name, _url, desc)) in crate::web_downloader::SAMPLE_MOVIES.iter().enumerate() {
            let title = NSString::from_str(&format!("{name} ({desc})"));
            let sample_block = RcBlock::new(move |_action: *mut AnyObject| {
                let _ = crate::web_downloader::add_sample_demo(idx);
            });
            let action: *mut AnyObject = unsafe {
                msg_send![
                    action_cls,
                    actionWithTitle: &*title,
                    style: 0isize,
                    handler: &*sample_block
                ]
            };
            unsafe { let _: () = msg_send![alert, addAction: action]; }
        }

        // Cancel
        let cancel_block = RcBlock::new(|_action: *mut AnyObject| {});
        let cancel_action: *mut AnyObject = unsafe {
            msg_send![
                action_cls,
                actionWithTitle: ns_string!("Hủy"),
                style: 1isize,
                handler: &*cancel_block
            ]
        };
        unsafe { let _: () = msg_send![alert, addAction: cancel_action]; }

        unsafe {
            let alert_vc: Retained<AnyObject> = Retained::retain(alert).unwrap();
            let _: () = msg_send![self, presentViewController: &*alert_vc, animated: true, completion: None::<&Block<dyn Fn()>>];
        }
    }

    #[allow(deprecated)]
    fn show_document_picker(&self) {
        tracing::info!("show document picker");
        let mtm = MainThreadMarker::from(self);
        let picker = UIDocumentPickerViewController::initWithDocumentTypes_inMode(
            mtm.alloc(),
            &NSArray::from_slice(&[ns_string!(RUF_UTI), ns_string!(SWF_UTI)]),
            UIDocumentPickerMode::Open,
        );
        picker.setDelegate(Some(ProtocolObject::from_ref(self)));
        // TODO: Consider setting picker.directoryURL to NSDownloadsDirectory,
        // as that's the likely place that people will have their SWFs.

        self.presentViewController_animated_completion(&picker, true, None);
    }

    fn toggle_editing(&self, button: &UIBarButtonItem) {
        let table_view = self.tableView().expect("has table view");
        let is_editing = !table_view.isEditing();
        table_view.setEditing_animated(is_editing, true);
        button.setTitle(Some(if is_editing {
            ns_string!("Done")
        } else {
            ns_string!("Edit")
        }));
    }

    fn configure_cell(&self, cell: &UITableViewCell, index_path: &NSIndexPath) {
        let subviews = cell.contentView().subviews();
        let title = subviews.objectAtIndex(1).downcast::<UILabel>().unwrap();
        let subtitle = subviews.objectAtIndex(2).downcast::<UILabel>().unwrap();

        let movie = unsafe { self.ivars().fetched_movies.objectAtIndexPath(index_path) };
        let cached_name = movie.cachedName();
        let url = movie.link();

        title.setText(Some(&cached_name));

        if url.isFileURL() {
            if let Some(_access) = storage::SecurityScopedResource::access(&url) {
                dbg!(&url, url.filePathURL());
                subtitle.setText(Some(
                    &url.filePathURL().unwrap().lastPathComponent().unwrap(),
                ));
            } else {
                subtitle.setText(Some(ns_string!("Unknown")));
            }
        } else {
            subtitle.setText(Some(&url.absoluteString().unwrap()));
        }
    }
}
